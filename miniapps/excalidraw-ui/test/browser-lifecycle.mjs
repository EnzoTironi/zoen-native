import { rm } from "node:fs/promises";

export async function withCleanup(body, cleanup) {
  let result, bodyError;
  let failed = false;
  try {
    result = await body();
  } catch (error) {
    bodyError = error;
    failed = true;
  }
  try {
    await cleanup();
  } catch (error) {
    if (failed)
      throw new AggregateError([bodyError, error], "Test body and owned cleanup both failed", {
        cause: bodyError,
      });
    throw error;
  }
  if (failed) throw bodyError;
  return result;
}

export function withBrowserProfile(folder, body) {
  return withCleanup(body, () =>
    // Node retries transient ENOTEMPTY/EBUSY on recursive removal with linear backoff:
    // https://nodejs.org/api/fs.html#fspromisesrmpath-options (at most 1.5 s here).
    rm(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }),
  );
}

export function observeOwnedChild(child, { processGroup = false } = {}) {
  const state = {
    pid: child.pid,
    exitCode: null,
    signalCode: null,
    closeObserved: false,
    stopped: false,
    spawnError: null,
  };
  // Register before any await: exit may precede cleanup, and inherited stdio may outlive exit.
  const closed = new Promise((resolve) => {
    child.once("error", (error) => {
      state.spawnError = error.message;
    });
    child.once("close", (code, signal) => {
      state.exitCode = code;
      state.signalCode = signal;
      state.closeObserved = true;
      state.stopped = code !== null || signal !== null || state.spawnError !== null;
      resolve();
    });
  });
  const groupAlive = () => {
    if (!processGroup || !child.pid) return false;
    try {
      process.kill(-child.pid, 0);
      return true;
    } catch (error) {
      if (error.code === "ESRCH") return false;
      throw error;
    }
  };
  const done = () => state.closeObserved && !groupAlive();
  const wait = async (ms) => {
    const until = performance.now() + ms;
    while (!done()) {
      const remaining = until - performance.now();
      if (remaining <= 0) return false;
      await new Promise((resolve) => setTimeout(resolve, Math.min(10, remaining)));
    }
    return true;
  };
  const signalErrors = [];
  const signal = (name) => {
    if (!child.pid) return;
    if (processGroup) {
      try {
        // Only the fresh detached child's own POSIX group, never discovered Chrome processes.
        process.kill(-child.pid, name);
      } catch (error) {
        // Browser.close may still finish after macOS refuses a fallback signal.
        // Keep observing within the same bound; EPERM never proves retirement.
        if (error.code === "EPERM") signalErrors.push({ signal: name, code: error.code });
        else if (error.code !== "ESRCH") throw error;
      }
    } else if (child.exitCode === null && child.signalCode === null) child.kill(name);
  };
  return {
    state,
    closed,
    async stop(graceful) {
      const deadline = performance.now() + 2000;
      const remaining = () => Math.max(0, deadline - performance.now());
      if (done()) return;
      if (graceful) {
        try {
          graceful();
        } catch {
          /* A closed CDP socket falls back to owned process signals. */
        }
        if (await wait(Math.min(200, remaining()))) return;
      }
      signal("SIGTERM");
      if (await wait(Math.min(800, remaining()))) return;
      signal("SIGKILL");
      if (await wait(remaining())) return;
      throw Error(
        "Owned Chrome did not close its process/stdio and retire its owned group within 2 s: " +
          JSON.stringify({
            ...state,
            exitCode: child.exitCode,
            signalCode: child.signalCode,
            signalErrors,
          }),
      );
    },
  };
}
