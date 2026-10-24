import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

/** Owned process lifecycle for the real Expo web acceptance journey. */
export function createJourneyProcesses(root) {
  const children = new Set();
  const processes = new Map();
  function start(command, args, options = {}) {
    const child = spawn(command, args, {
      cwd: root,
      ...options,
      detached: globalThis.process.platform !== "win32",
      stdio: ["ignore", "pipe", "pipe"],
    });
    children.add(child);
    let stdout = "",
      stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout = `${stdout}${chunk}`.slice(-2_000_000);
    });
    child.stderr.on("data", (chunk) => {
      stderr = `${stderr}${chunk}`.slice(-2_000_000);
    });
    const done = new Promise((resolveDone) => {
      child.once("error", (error) => {
        children.delete(child);
        stderr += error.message;
        resolveDone({ code: -1, signal: null, stdout, stderr });
      });
      child.once("exit", (code, signal) => {
        children.delete(child);
        resolveDone({ code, signal, stdout, stderr });
      });
    });
    const process = { child, done, output: () => stdout + stderr };
    processes.set(child, process);
    return process;
  }
  async function stop(process) {
    if (!process || process.child.exitCode !== null || process.child.signalCode !== null) return;
    const signal = (value) => {
      try {
        if (globalThis.process.platform !== "win32" && process.child.pid)
          globalThis.process.kill(-process.child.pid, value);
        else process.child.kill(value);
      } catch (error) {
        if (error.code !== "ESRCH") throw error;
      }
    };
    signal("SIGINT");
    if (
      !(await Promise.race([process.done.then(() => true), delay(3000, false, { ref: false })]))
    ) {
      signal("SIGKILL");
      await process.done;
    }
  }

  return { children, processes, start, stop };
}
