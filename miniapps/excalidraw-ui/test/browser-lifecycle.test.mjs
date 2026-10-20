import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { observeOwnedChild, withCleanup } from "./browser-lifecycle.mjs";

function ownedNode(source, stdio = ["ignore", "ignore", "pipe"]) {
  const child = spawn(process.execPath, ["-e", source], {
    stdio,
    detached: process.platform !== "win32",
  });
  return { child, owned: observeOwnedChild(child, { processGroup: process.platform !== "win32" }) };
}

test(
  "close waits for a real inherited pipe and its profile write after the leader exits",
  { timeout: 5000 },
  async () => {
    const folder = await mkdtemp(join(tmpdir(), "zoen-browser-close-"));
    const marker = join(folder, "last-profile-write");
    const descendant = `setTimeout(() => { require('node:fs').writeFileSync(${JSON.stringify(marker)}, 'closed'); process.stderr.write('last-write'); }, 300)`;
    const { child, owned } = ownedNode(`
    const child = require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(descendant)}], {stdio:['ignore','ignore','inherit']});
    child.unref();
    process.exit(0);
  `);
    child.stderr.resume();
    try {
      await once(child, "exit");
      assert.equal(child.exitCode, 0);
      assert.equal(
        owned.state.closeObserved,
        false,
        "leader exit cannot authorize profile removal",
      );
      await owned.closed;
      assert.equal(await readFile(marker, "utf8"), "closed");
      assert.equal(owned.state.closeObserved, true);
      assert.equal(owned.state.stopped, true);
      await owned.stop(); // A close event that already fired cannot be missed by cleanup.
    } finally {
      await owned.stop();
      await rm(folder, { recursive: true, force: true });
    }
  },
);

test(
  "an already signaled exit is terminal after close, despite a null exit code",
  { timeout: 5000 },
  async () => {
    const { child, owned } = ownedNode("setInterval(() => {}, 1000)");
    child.stderr.resume();
    try {
      const exited = once(child, "exit");
      child.kill("SIGTERM");
      await exited;
      await owned.closed;
      assert.equal(child.exitCode, null);
      assert.equal(child.signalCode, "SIGTERM");
      await owned.stop();
      assert.equal(owned.state.signalCode, "SIGTERM");
      assert.equal(owned.state.closeObserved, true);
      assert.equal(owned.state.stopped, true);
    } finally {
      await owned.stop();
    }
  },
);

test(
  "graceful shutdown closes the actual child without process signals",
  { timeout: 5000 },
  async () => {
    const { child, owned } = ownedNode(
      "process.stdin.resume(); process.stdin.once('data', () => process.exit(0))",
      ["pipe", "ignore", "pipe"],
    );
    child.stderr.resume();
    try {
      await once(child, "spawn");
      await owned.stop(() => child.stdin.end("close"));
      assert.equal(owned.state.exitCode, 0);
      assert.equal(owned.state.signalCode, null);
      assert.equal(owned.state.closeObserved, true);
    } finally {
      await owned.stop();
    }
  },
);

test(
  "owned POSIX group fallback closes inherited pipes after the leader exits",
  { skip: process.platform === "win32", timeout: 5000 },
  async () => {
    const descendant = "setInterval(() => {}, 1000)";
    const { child, owned } = ownedNode(`
    const child = require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(descendant)}], {stdio:['ignore','ignore','inherit']});
    child.unref();
    process.exit(0);
  `);
    child.stderr.resume();
    try {
      await once(child, "exit");
      assert.equal(owned.state.closeObserved, false);
      await owned.stop();
      assert.equal(owned.state.closeObserved, true);
      assert.equal(owned.state.stopped, true);
    } finally {
      await owned.stop();
    }
  },
);

test(
  "owned group cleanup also retires a descendant with no inherited pipes",
  { skip: process.platform === "win32", timeout: 5000 },
  async () => {
    const folder = await mkdtemp(join(tmpdir(), "zoen-browser-group-"));
    const ready = join(folder, "ready");
    const retired = join(folder, "retired");
    const descendant = `
    process.on('SIGTERM', () => { require('node:fs').writeFileSync(${JSON.stringify(retired)}, 'retired'); process.exit(0); });
    require('node:fs').writeFileSync(${JSON.stringify(ready)}, 'ready');
    setInterval(() => {}, 1000);
  `;
    const { child, owned } = ownedNode(`
    const child = require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(descendant)}], {stdio:'ignore'});
    child.unref();
    setInterval(() => { if(require('node:fs').existsSync(${JSON.stringify(ready)})) process.exit(0); }, 10);
  `);
    child.stderr.resume();
    try {
      await owned.closed;
      assert.equal(await readFile(ready, "utf8"), "ready");
      assert.equal(owned.state.closeObserved, true, "leader and its stdio already closed");
      await owned.stop();
      assert.equal(
        await readFile(retired, "utf8"),
        "retired",
        "closed leader must not skip its live owned group",
      );
      assert.throws(() => process.kill(-child.pid, 0), { code: "ESRCH" });
    } finally {
      await owned.stop();
      await rm(folder, { recursive: true, force: true });
    }
  },
);

test("cleanup preserves the exact primary failure and reports both failures", async () => {
  const primary = Error("original renderer assertion");
  const cleanup = Error("profile removal exhausted retries");
  await assert.rejects(
    withCleanup(
      () => {
        throw primary;
      },
      () => {
        throw cleanup;
      },
    ),
    (error) => {
      assert.ok(error instanceof AggregateError);
      assert.equal(error.cause, primary);
      assert.deepEqual(error.errors, [primary, cleanup]);
      return true;
    },
  );
  await assert.rejects(
    withCleanup(
      () => {
        throw primary;
      },
      () => {},
    ),
    (error) => error === primary,
  );
  await assert.rejects(
    withCleanup(
      () => "body passed",
      () => {
        throw cleanup;
      },
    ),
    (error) => error === cleanup,
  );
  assert.equal(
    await withCleanup(
      () => "body passed",
      () => {},
    ),
    "body passed",
  );
});
