// Actual local candidate process for the native dual-wire oracle. This imports
// production entrypoints only. Process IPC controls restart; HTTP has no test API.
import assert from "node:assert/strict";
import { readFile, realpath, mkdir } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

assert.equal(typeof process.send, "function", "Start only as an owned IPC child");
const root = fileURLToPath(new URL("../", import.meta.url));
const run = await realpath(process.env.ZOEN_WIRE_RUN_DIR);
assert.equal(dirname(run), "/private/tmp/zoen-relay-wire-v107");
assert.match(run.split("/").at(-1), /^run_[a-zA-Z0-9_-]+$/);
const inventory = JSON.parse(await readFile(join(run, "candidate-ownership.json"), "utf8"));
assert.equal(inventory.run_dir, run);
assert.equal(inventory.parent_pid, process.ppid);
assert.match(inventory.directory_cell, /^qa_wire_v107_[a-z0-9_]+$/);
const persistence = join(run, "candidate");
await mkdir(persistence, { recursive: true, mode: 0o700 });
assert.equal(await realpath(persistence), persistence);

async function modules(entry) {
  const seen = new Set();
  const result = [];
  async function visit(path) {
    path = await realpath(path);
    assert.ok(!relative(root, path).startsWith(".."), "Source must remain inside relay-state");
    if (seen.has(path)) return;
    seen.add(path);
    if (path.endsWith(".wasm")) {
      result.push({ type: "CompiledWasm", path });
      return;
    }
    assert.match(path, /\.m?js$/);
    result.push({ type: "ESModule", path });
    const source = await readFile(path, "utf8");
    for (const match of source.matchAll(/\b(?:from|import)\s*["'](\.[^"']+)["']/g))
      await visit(resolve(dirname(path), match[1]));
  }
  await visit(join(root, entry));
  return result;
}
const settings = {
  RELAY_PROTOCOL_ENABLED: "true",
  RELAY_ALLOWED_ORIGINS: '["https://app.tryzoen.com"]',
};
let runtime;
let port = 0;
let generation = 0;
let busy = false;

async function start() {
  assert.equal(runtime, undefined);
  const workers = [
    {
      name: "ingress",
      modules: await modules("src/relay-index.mjs"),
      bindings: settings,
      serviceBindings: { DIRECTORY_RELAY: { name: "directory", entrypoint: "RelayIngress" } },
    },
    {
      name: "directory",
      modules: await modules("src/directory-index.mjs"),
      bindings: {
        ...settings,
        DIRECTORY_CELL: inventory.directory_cell,
        RELAY_NAME: "qa-wire-v107",
      },
      durableObjects: { DIRECTORY: { className: "DirectoryState", useSQLite: true } },
      serviceBindings: { CLOSED_RELAY: "closed" },
    },
    {
      name: "closed",
      modules: await modules("src/closed-index.mjs"),
      durableObjects: { CLOSED_SPACE: { className: "ClosedSpace", useSQLite: true } },
      serviceBindings: { RELAY_DIRECTORY: { name: "directory", entrypoint: "RelayDispatch" } },
    },
  ].map((worker) => ({
    ...worker,
    modulesRoot: root,
    compatibilityDate: "2026-10-18",
  }));
  runtime = new Miniflare(
    convertV4MiniflareOptions({
      workers,
      host: "127.0.0.1",
      port,
      // Without shared storage, Miniflare5 derives its effective isolated root
      // from resourcePersistencePath. Setting only the isolated path is ignored.
      resourcePersistencePath: persistence,
    }),
  );
  const address = await runtime.ready;
  assert.equal(address.hostname, "127.0.0.1");
  assert.ok(![8790, 5547].includes(Number(address.port)));
  if (port) assert.equal(Number(address.port), port, "Restart must preserve the actual endpoint");
  port = Number(address.port);
  address.pathname = "/v1/sync";
  address.protocol = "ws:";
  process.send({
    state: "ready",
    generation: ++generation,
    endpoint: address.href,
    directory_cell: inventory.directory_cell,
    persistence,
    scope: "actual local candidate only; no public activation or qualification",
  });
}
async function stop() {
  const current = runtime;
  runtime = undefined;
  if (current) await current.dispose();
}
process.on("message", async (message) => {
  try {
    assert.equal(busy, false, "One parent control operation at a time");
    busy = true;
    assert.equal(message?.run_dir, run);
    assert.deepEqual(Object.keys(message).sort(), ["op", "run_dir"]);
    assert.ok(["restart", "stop"].includes(message.op));
    await stop();
    if (message.op === "restart") await start();
    else {
      process.send({ state: "stopped", generation });
      process.disconnect();
    }
  } catch (error) {
    process.send?.({ state: "failed", error: String(error) });
    await stop();
    process.exitCode = 1;
    process.disconnect();
  } finally {
    busy = false;
  }
});
process.once("disconnect", () => {
  void stop().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
});
try {
  await start();
} catch (error) {
  process.send?.({ state: "failed", error: String(error) });
  await stop();
  process.exitCode = 1;
  process.disconnect();
}
