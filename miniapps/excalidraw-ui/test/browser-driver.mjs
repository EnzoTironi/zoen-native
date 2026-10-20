import { spawn } from "node:child_process";
import { observeOwnedChild, withCleanup } from "./browser-lifecycle.mjs";
import { constants } from "node:fs";
import { access, stat } from "node:fs/promises";
import { join, posix, win32 } from "node:path";
async function isExecutableFile(path) {
  try {
    if (!(await stat(path)).isFile()) return false;
    await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

export async function findBrowserExecutable({
  env = process.env,
  platform = process.platform,
  canExecute = isExecutableFile,
} = {}) {
  const paths = platform === "win32" ? win32 : posix;
  const directories = (env.PATH ?? "").split(platform === "win32" ? ";" : ":").filter(Boolean);
  const fromPath = async (names) => {
    for (const name of names) {
      for (const directory of directories) {
        const candidate = paths.resolve(directory, name);
        if (await canExecute(candidate)) return candidate;
      }
    }
    return null;
  };
  const explicit = env.CHROME_BIN?.trim();
  if (explicit) {
    const candidate = /[/\\]/.test(explicit) ? paths.resolve(explicit) : await fromPath([explicit]);
    if (candidate && (await canExecute(candidate))) return candidate;
    throw new Error(`CHROME_BIN is not an executable browser file: ${explicit}`);
  }
  const known =
    platform === "darwin"
      ? [
          "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
          "/Applications/Chromium.app/Contents/MacOS/Chromium",
          ...(env.HOME
            ? [paths.join(env.HOME, "Applications/Google Chrome.app/Contents/MacOS/Google Chrome")]
            : []),
        ]
      : platform === "win32"
        ? [env.PROGRAMFILES, env["PROGRAMFILES(X86)"], env.LOCALAPPDATA]
            .filter(Boolean)
            .map((directory) => paths.join(directory, "Google/Chrome/Application/chrome.exe"))
        : [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/opt/google/chrome/chrome",
          ];
  for (const candidate of known) if (await canExecute(candidate)) return candidate;
  const candidate = await fromPath(
    platform === "win32"
      ? ["chrome.exe", "chromium.exe"]
      : ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"],
  );
  if (candidate) return candidate;
  const required = env.GITHUB_ACTIONS === "true" || Boolean(env.CI && !/^(false|0)$/i.test(env.CI));
  if (required) {
    throw new Error(
      "Chrome/Chromium is required for CI renderer tests. Install its normal sandbox runtime or set CHROME_BIN to an executable browser.",
    );
  }
  return null;
}

/** Fixed, independent phase budgets: slow process startup cannot consume renderer time. */
export function createBrowserDeadline({ executionMilliseconds = 18000 } = {}) {
  let phase = "startup";
  let reject;
  const expired = new Promise((_, fail) => {
    reject = fail;
  });
  const arm = (milliseconds, message) =>
    setTimeout(() => {
      phase = "expired";
      reject(Error(message));
    }, milliseconds);
  let timer = arm(30000, "Owned Chrome startup timeout");
  return {
    expired,
    startExecution() {
      // stderr may repeat the DevTools endpoint; it must not extend either budget.
      if (phase !== "startup") return;
      clearTimeout(timer);
      phase = "execution";
      timer = arm(executionMilliseconds, "Owned Chrome test deadline");
    },
    clear() {
      phase = "cleared";
      clearTimeout(timer);
    },
  };
}

export async function ownedBrowserResult(chrome, folder, path) {
  const child = spawn(
    chrome,
    [
      "--headless",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-sync",
      "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost",
      "--remote-debugging-address=127.0.0.1",
      "--remote-debugging-port=0",
      `--user-data-dir=${join(folder, "profile")}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"], detached: process.platform !== "win32" },
  );
  const owned = observeOwnedChild(child, { processGroup: process.platform !== "win32" });
  const lifecycle = Object.assign(owned.state, { profile: join(folder, "profile") });
  let socket, closeBrowser;
  const deadline = createBrowserDeadline();
  let stderr = "";
  const errors = [];
  return withCleanup(async () => {
    return await Promise.race([
      (async () => {
        const endpoint = await new Promise((resolve, reject) => {
          child.once("error", reject);
          child.once("exit", () => {
            reject(Error("Owned Chrome exited"));
          });
          child.stderr.on("data", (v) => {
            stderr = (stderr + v).slice(-16384);
            const match = stderr.match(/DevTools listening on (ws:\/\/127\.0\.0\.1:[^\s]+)/);
            if (match) {
              deadline.startExecution();
              resolve(match[1]);
            }
          });
        });
        socket = new WebSocket(endpoint);
        await new Promise((resolve, reject) => {
          socket.addEventListener("open", resolve, { once: true });
          socket.addEventListener("error", reject, { once: true });
        });
        let next = 0;
        const pending = new Map();
        socket.addEventListener("message", (event) => {
          const packet = JSON.parse(String(event.data));
          if (packet.id !== undefined) {
            const task = pending.get(packet.id);
            pending.delete(packet.id);
            if (packet.error) task?.reject(Error(packet.error.message));
            else task?.resolve(packet.result ?? {});
          } else if (packet.method === "Runtime.consoleAPICalled" && packet.params.type === "error")
            errors.push(packet.params.args.map((a) => a.value ?? a.description).join(" "));
          else if (packet.method === "Runtime.exceptionThrown")
            errors.push(
              packet.params.exceptionDetails.exception?.description ??
                packet.params.exceptionDetails.text,
            );
        });
        const send = (method, params = {}, sessionId) =>
          new Promise((resolve, reject) => {
            const id = ++next;
            pending.set(id, { resolve, reject });
            socket.send(
              JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }),
            );
          });
        closeBrowser = () => {
          if (socket.readyState === WebSocket.OPEN)
            socket.send(JSON.stringify({ id: ++next, method: "Browser.close" }));
        };
        const target = await send("Target.createTarget", { url: "about:blank" });
        const attached = await send("Target.attachToTarget", {
          targetId: target.targetId,
          flatten: true,
        });
        const session = attached.sessionId;
        await send(
          "Target.setAutoAttach",
          { autoAttach: true, waitForDebuggerOnStart: false, flatten: true },
          session,
        );
        await send("Runtime.enable", {}, session);
        await send("Page.enable", {}, session);
        await send("Page.navigate", { url: `file://${path}` }, session);
        // Poll in real browser time: virtual-time dump-dom may finish before opaque child rendering.
        for (let at = 0; at < 100; at++) {
          const result = await send(
            "Runtime.evaluate",
            {
              expression: "document.getElementById('result')?.textContent??null",
              returnByValue: true,
            },
            session,
          );
          if (typeof result.result?.value === "string")
            return {
              text: result.result.value,
              errors,
              lifecycle,
              png: (await send("Page.captureScreenshot", { format: "png" }, session)).data,
            };
          await new Promise((r) => setTimeout(r, 100));
        }
        const info = await send(
          "Runtime.evaluate",
          {
            expression: "({body:document.body.innerText.slice(0,300),messages:window.qaMessages})",
            returnByValue: true,
          },
          session,
        );
        throw Error("Real canvas timeout " + JSON.stringify({ errors, info: info.result?.value }));
      })(),
      deadline.expired,
    ]);
  }, async () => {
    deadline.clear();
    try {
      await owned.stop(closeBrowser);
    } finally {
      socket?.close();
    }
  }).catch((error) => {
    error.ownedBrowser = { ...lifecycle, stderr };
    throw error;
  });
}
