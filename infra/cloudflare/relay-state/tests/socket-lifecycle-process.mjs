// POSIX owned-child crash qualification only; never imported by Workers.
import assert from "node:assert/strict";
import { fork, execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { closeSync, openSync } from "node:fs";
import { mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const childPath = fileURLToPath(new URL("./socket-lifecycle-child.mjs", import.meta.url));
function bounded(promise, reason, ms = 5000) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(reason)), ms);
    }),
  ]).finally(() => clearTimeout(timer));
}
export async function createOwnedRun() {
  assert.notEqual(
    process.platform,
    "win32",
    "Actual owned process-group crash proof requires POSIX",
  );
  const runDir = await realpath(await mkdtemp(join(tmpdir(), "zoen-ordinary-restart-")));
  const marker = {
    schema: 1,
    run_dir: runDir,
    parent_pid: process.pid,
    token: randomBytes(32).toString("hex"),
  };
  await writeFile(join(runDir, "ownership.json"), JSON.stringify(marker), {
    flag: "wx",
    mode: 0o600,
  });
  return { runDir, marker, children: [], generation: 0 };
}
async function verifyMarker(run) {
  assert.equal(await realpath(run.runDir), run.runDir);
  assert.equal(dirname(run.runDir), await realpath(tmpdir()));
  assert.match(basename(run.runDir), /^zoen-ordinary-restart-[A-Za-z0-9_-]+$/);
  assert.deepEqual(
    JSON.parse(await readFile(join(run.runDir, "ownership.json"), "utf8")),
    run.marker,
  );
  assert.equal(run.marker.parent_pid, process.pid);
}
function inventory() {
  // Never print full machine commands. Keep only owned group metadata in evidence.
  return execFileSync("ps", ["-ww", "-axo", "pid=,ppid=,pgid=,stat=,command="], {
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
  })
    .trim()
    .split("\n")
    .map((line) => {
      const match = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/);
      assert.ok(match, "Expected process inventory shape");
      return {
        pid: Number(match[1]),
        parent: Number(match[2]),
        group: Number(match[3]),
        state: match[4],
        command: match[5],
      };
    });
}
async function ownedGroup(run, instance) {
  await verifyMarker(run);
  assert.ok(Number.isSafeInteger(instance.child.pid) && instance.child.pid > 1);
  assert.notEqual(instance.child.pid, process.pid);
  const processes = inventory();
  const leader = processes.find((entry) => entry.pid === instance.child.pid);
  assert.ok(leader, "Owned group leader must still exist before any signal");
  assert.equal(leader.parent, process.pid);
  assert.equal(leader.group, instance.child.pid, "Detached fork must own its new group");
  assert.ok(
    leader.command.includes(childPath),
    "PID must still be the exact owned lifecycle child",
  );
  const members = processes.filter((entry) => entry.group === instance.child.pid);
  const byId = new Map(processes.map((entry) => [entry.pid, entry]));
  for (const member of members) {
    let current = member;
    for (let hops = 0; current.pid !== leader.pid; hops++) {
      assert.ok(hops < 64, "Owned descendants must have a bounded ancestry");
      current = byId.get(current.parent);
      assert.ok(current, "Every signaled member must descend from the exact owned child");
    }
  }
  return members.map(({ pid, parent, group, state }) => ({ pid, parent, group, state }));
}
async function groupEnded(pid) {
  const until = Date.now() + 5000;
  do {
    if (!inventory().some((entry) => entry.group === pid && !entry.state.startsWith("Z"))) return;
    await new Promise((accept) => setTimeout(accept, 20));
  } while (Date.now() < until);
  assert.fail("An owned runtime process remains alive after termination");
}
export async function startOwnedChild(run) {
  await verifyMarker(run);
  const generation = ++run.generation;
  const logs = ["stdout", "stderr"].map((kind) =>
    openSync(join(run.runDir, `child-${generation}.${kind}.log`), "wx", 0o600),
  );
  let child;
  try {
    child = fork(childPath, [], {
      detached: true,
      execArgv: [],
      env: {
        ...process.env,
        NODE_OPTIONS: undefined,
        VSCODE_INSPECTOR_OPTIONS: undefined,
        NODE_INSPECTOR_INFO: undefined,
        ZOEN_LIFECYCLE_RUN_DIR: run.runDir,
        ZOEN_LIFECYCLE_TOKEN: run.marker.token,
      },
      stdio: ["ignore", ...logs, "ipc"],
    });
  } finally {
    for (const fd of logs) closeSync(fd);
  }
  const exit = new Promise((accept) =>
    child.once("exit", (code, signal) => accept({ code, signal })),
  );
  const instance = { child, exit, generation, ended: false };
  run.children.push(instance);
  const ready = await bounded(
    new Promise((accept, reject) => {
      child.once("error", reject);
      child.on("message", (message) => {
        if (message?.state === "failed") reject(new Error(message.error));
        else if (message?.state === "ready") accept(message);
      });
      child.once("exit", () => reject(new Error("Owned lifecycle child exited before readiness")));
    }),
    "Owned runtime startup failed",
    15_000,
  ).catch(async (error) => {
    // Signal only a verified still-live owned leader. An unverified orphan is a
    // retained failed-cleanup artifact, never permission to kill arbitrary PIDs.
    if (child.exitCode === null && child.signalCode === null) {
      await ownedGroup(run, { child });
      process.kill(-child.pid, "SIGKILL");
      await bounded(exit, "Owned startup cleanup failed");
    }
    await groupEnded(child.pid);
    instance.ended = true;
    throw error;
  });
  assert.equal(ready.pid, child.pid);
  assert.equal(ready.run_dir, run.runDir);
  assert.equal(ready.parent_pid, process.pid);
  const endpoint = new URL(ready.endpoint);
  assert.equal(endpoint.hostname, "127.0.0.1");
  assert.ok(![8790, 5547].includes(Number(endpoint.port)));
  instance.endpoint = endpoint.origin.replace("ws:", "http:");
  await ownedGroup(run, instance);
  return instance;
}
export async function crashOwnedChild(run, instance) {
  const group = await ownedGroup(run, instance);
  await writeFile(
    join(run.runDir, `child-${instance.generation}.owned-group.json`),
    JSON.stringify(group, null, 2),
    { flag: "wx" },
  );
  // No graceful dispose or synthetic close: kill only this verified fresh group.
  process.kill(-instance.child.pid, "SIGKILL");
  const ended = await bounded(instance.exit, "Owned abrupt runtime stop failed");
  assert.equal(ended.signal, "SIGKILL");
  await groupEnded(instance.child.pid);
  instance.ended = true;
}
export async function stopOwnedChild(run, instance) {
  if (instance.ended) return;
  await ownedGroup(run, instance);
  const stopped = new Promise((accept, reject) => {
    const listener = (message) => {
      if (message?.state === "stopped") {
        instance.child.off("message", listener);
        accept();
      } else if (message?.state === "failed") reject(new Error(message.error));
    };
    instance.child.on("message", listener);
  });
  instance.child.send({ op: "stop", run_dir: run.runDir, token: run.marker.token });
  try {
    await bounded(stopped, "Owned runtime graceful cleanup failed", 15_000);
    assert.deepEqual(await bounded(instance.exit, "Owned lifecycle child did not exit"), {
      code: 0,
      signal: null,
    });
  } catch (error) {
    if (instance.child.exitCode === null && instance.child.signalCode === null) {
      await ownedGroup(run, instance);
      process.kill(-instance.child.pid, "SIGKILL");
      await bounded(instance.exit, "Owned fallback cleanup failed");
    }
    throw error;
  }
  await groupEnded(instance.child.pid);
  instance.ended = true;
}
