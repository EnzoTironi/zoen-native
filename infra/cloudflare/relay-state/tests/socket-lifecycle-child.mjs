// Dedicated real workerd child; private IPC owns lifecycle, no HTTP close/fault controls.
import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import { join, dirname, basename } from "node:path";
import { tmpdir } from "node:os";
import { runtime } from "./socket-lifecycle-harness.mjs";
assert.equal(typeof process.send, "function", "Only an owned fork may start this runtime");
const run = await realpath(process.env.ZOEN_LIFECYCLE_RUN_DIR);
assert.equal(dirname(run), await realpath(tmpdir()));
assert.match(basename(run), /^zoen-ordinary-restart-[A-Za-z0-9_-]+$/);
const marker = JSON.parse(await readFile(join(run, "ownership.json"), "utf8"));
assert.equal(marker.schema, 1);
assert.equal(marker.run_dir, run);
assert.equal(marker.parent_pid, process.ppid);
assert.equal(marker.token, process.env.ZOEN_LIFECYCLE_TOKEN);
let mf;
let stopping = false;
async function stop() {
  const current = mf;
  mf = undefined;
  if (current) await current.dispose();
}
process.on("message", async (message) => {
  try {
    assert.equal(stopping, false);
    assert.deepEqual(Object.keys(message).sort(), ["op", "run_dir", "token"]);
    assert.equal(message.op, "stop");
    assert.equal(message.run_dir, run);
    assert.equal(message.token, marker.token);
    stopping = true;
    await stop();
    process.send({ state: "stopped" });
    process.disconnect();
  } catch (error) {
    process.send?.({ state: "failed", error: String(error) });
    await stop();
    process.exitCode = 1;
    process.disconnect();
  }
});
process.once("disconnect", () => {
  void stop().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
});
try {
  mf = await runtime(join(run, "persistence"));
  const endpoint = new URL("/socket", await mf.ready);
  endpoint.protocol = "ws:";
  process.send({
    state: "ready",
    pid: process.pid,
    parent_pid: process.ppid,
    run_dir: run,
    endpoint: endpoint.href,
  });
} catch (error) {
  process.send?.({ state: "failed", error: String(error) });
  await stop();
  process.exitCode = 1;
  process.disconnect();
}
