import { spawn } from "node:child_process";
import { join } from "node:path";
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
