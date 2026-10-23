import { spawn } from "node:child_process";
import { observeOwnedChild, withCleanup } from "./browser-lifecycle.mjs";
import { join } from "node:path";
export { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";

/** Fixed, independent phase budgets: slow process startup cannot consume renderer time. */
export function createBrowserDeadline() {
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
      timer = arm(18000, "Owned Chrome test deadline");
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
