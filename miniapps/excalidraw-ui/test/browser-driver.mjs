import { spawn } from "node:child_process";
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
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  let socket, deadline;
  const errors = [];
  const lifecycle = {
    pid: child.pid,
    profile: join(folder, "profile"),
    exitCode: null,
    stopped: false,
  };
  try {
    return await Promise.race([
      (async () => {
        const endpoint = await new Promise((resolve, reject) => {
          let output = "";
          const timer = setTimeout(() => reject(Error("Owned Chrome startup timeout")), 10000);
          child.once("error", reject);
          child.once("exit", () => {
            clearTimeout(timer);
            reject(Error("Owned Chrome exited"));
          });
          child.stderr.on("data", (v) => {
            output = (output + v).slice(-16384);
            const match = output.match(/DevTools listening on (ws:\/\/127\.0\.0\.1:[^\s]+)/);
            if (match) {
              clearTimeout(timer);
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
      new Promise((_, reject) => {
        deadline = setTimeout(() => reject(Error("Owned Chrome test deadline")), 18000);
      }),
    ]);
  } finally {
    clearTimeout(deadline);
    socket?.close();
    if (child.exitCode === null) {
      const ended = new Promise((resolve) => child.once("exit", resolve));
      child.kill("SIGTERM");
      const timer = setTimeout(() => child.kill("SIGKILL"), 2000);
      await ended;
      clearTimeout(timer);
    }
    lifecycle.exitCode = child.exitCode;
    lifecycle.stopped = child.exitCode !== null;
  }
}
