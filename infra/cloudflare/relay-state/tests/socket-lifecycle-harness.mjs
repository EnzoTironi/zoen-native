// Actual ordinary workerd/TCP lifecycle with native codec/certified-device frames.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir, realpath, stat, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
const root = fileURLToPath(new URL("../", import.meta.url));
export const native = JSON.parse(
  await readFile(
    process.env.ZOEN_DIRECTORY_FIXTURE
      ? resolve(process.env.ZOEN_DIRECTORY_FIXTURE)
      : join(root, "generated/directory-fixture.json"),
    "utf8",
  ),
);
export const named = (label) => {
  const step = native.steps.find((entry) => entry.label === label);
  assert.ok(step, `Require actual native fixture ${label}`);
  return step;
};
export const binary = (value) => Buffer.from(value, "base64");
async function modules(entry) {
  const seen = new Set(),
    result = [];
  async function visit(path) {
    path = await realpath(path);
    assert.ok(!relative(root, path).startsWith(".."), "Modules remain in relay-state");
    if (seen.has(path)) return;
    seen.add(path);
    result.push({ type: path.endsWith(".wasm") ? "CompiledWasm" : "ESModule", path });
    if (path.endsWith(".wasm")) return;
    for (const match of (await readFile(path, "utf8")).matchAll(
      /\b(?:from|import)\s*["'](\.[^"']+)["']/g,
    ))
      await visit(resolve(dirname(path), match[1]));
  }
  await visit(join(root, entry));
  return result;
}
export async function runtime(persistence) {
  return new Miniflare(
    convertV4MiniflareOptions({
      host: "127.0.0.1",
      port: 0,
      ...(persistence ? { resourcePersistencePath: persistence } : {}),
      workers: [
        {
          name: "directory",
          modules: await modules("tests/directory-lifecycle-fixture.mjs"),
          bindings: {
            RELAY_NAME: "zoen-cloudflare-private",
            RELAY_PROTOCOL_ENABLED: "true",
            DIRECTORY_CELL: "ordinary-lifecycle-owned",
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
      ].map((worker) => ({ ...worker, modulesRoot: root, compatibilityDate: "2026-10-18" })),
    }),
  );
}
export async function call(mf, path, value) {
  const response = await mf.dispatchFetch(`https://owned-lifecycle.test${path}`, {
    method: "POST",
    body: JSON.stringify(value ?? {}),
  });
  assert.equal(response.status, 200, `Local lifecycle operation ${path}`);
  return response.json();
}
export function bounded(promise, reason, timeout = 5000) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(reason)), timeout);
    }),
  ]).finally(() => clearTimeout(timer));
}
export async function eventually(mf, predicate, reason) {
  const until = Date.now() + 5000;
  let state;
  do {
    state = await call(mf, "/inspect");
    if (predicate(state)) return state;
    await new Promise((accept) => setTimeout(accept, 20));
  } while (Date.now() < until);
  assert.fail(`${reason}: ${JSON.stringify(state)}`);
}
export async function connect(mf, login) {
  await call(mf, "/configure", { nonce: login.nonce });
  const address = new URL("/socket", await mf.ready);
  address.protocol = "ws:";
  const socket = new WebSocket(address);
  socket.binaryType = "arraybuffer";
  const messages = [],
    waiters = [];
  let ended = false;
  const closed = new Promise((accept) => {
    socket.addEventListener("close", (event) => {
      ended = true;
      accept({ code: event.code });
      for (const waiter of waiters.splice(0)) waiter.reject(new Error("Actual socket ended"));
    });
  });
  socket.addEventListener("message", (event) => {
    assert.notEqual(typeof event.data, "string");
    const bytes = Buffer.from(event.data),
      waiting = waiters.shift();
    if (waiting) waiting.accept(bytes);
    else messages.push(bytes);
  });
  const next = () => {
    if (messages.length) return Promise.resolve(messages.shift());
    if (ended) return Promise.reject(new Error("Actual socket already ended"));
    return bounded(
      new Promise((accept, reject) => waiters.push({ accept, reject })),
      "Expected actual native binary frame",
    );
  };
  await bounded(
    new Promise((accept, reject) => {
      socket.addEventListener("open", accept, { once: true });
      socket.addEventListener("error", reject, { once: true });
    }),
    "Expected real loopback connection",
  );
  socket.send(binary(login.hello_base64));
  assert.deepEqual(await next(), binary(login.challenge_base64), "Actual native Challenge bytes");
  socket.send(binary(login.auth_base64));
  assert.deepEqual(await next(), binary(login.ready_base64), "Actual native Ready bytes");
  return {
    socket,
    next,
    messages,
    closed: () => bounded(closed, "Expected genuine runtime terminal close"),
  };
}
export async function register(mf, channel) {
  const step = named("register-first");
  channel.socket.send(binary(step.frame_base64));
  assert.deepEqual(await channel.next(), binary(step.expected.reply_base64));
  return eventually(
    mf,
    (state) =>
      state.attached === 1 &&
      state.online === 0 &&
      state.offline === 0 &&
      state.nextAlarm > Date.now() + 60_000,
    "Initial presence work must settle before the fault/restart",
  );
}
export async function emptySync(channel) {
  // This proves transport/catch-up completion for an empty catalogue only.
  // Real committed MLS history recovery is the separate 520-message dual-wire gate.
  channel.socket.send(binary(native.transport.unsupported_space_base64));
  assert.deepEqual(await channel.next(), binary(native.transport.sync_done_base64));
}
export async function actualRestartReconnect() {
  const { createOwnedRun, startOwnedChild, crashOwnedChild, stopOwnedChild } =
    await import("./socket-lifecycle-process.mjs");
  const run = await createOwnedRun();
  const remote = (instance) => ({
    ready: Promise.resolve(new URL(instance.endpoint)),
    dispatchFetch: (address, options) =>
      fetch(new URL(new URL(address).pathname, instance.endpoint), {
        ...options,
        signal: AbortSignal.timeout(5000),
      }),
  });
  let passed = false;
  try {
    const firstProcess = await startOwnedChild(run);
    const firstRuntime = remote(firstProcess);
    const first = await connect(firstRuntime, named("login-first"));
    const registered = await register(firstRuntime, first);
    assert.equal(registered.identities, 1);
    assert.equal(registered.devices, 1);
    await emptySync(first);
    const persistence = join(run.runDir, "persistence");
    const files = await readdir(persistence, { recursive: true });
    const sqlite = await Promise.all(
      files
        .filter((name) => name.endsWith(".sqlite"))
        .map(async (name) => ({ name, bytes: (await stat(join(persistence, name))).size })),
    );
    assert.ok(
      sqlite.some((file) => file.bytes > 0),
      "Actual nonempty SQLite persistence must exist before abrupt runtime death",
    );
    const before = await call(firstRuntime, "/inspect");
    assert.equal(before.connections, 1);
    assert.equal(before.ready, 1);
    assert.equal(before.sessions, 1);
    assert.equal(before.attached, 1);
    await writeFile(
      join(run.runDir, "before-crash.json"),
      JSON.stringify({ before, sqlite }, null, 2),
      { flag: "wx" },
    );
    await crashOwnedChild(run, firstProcess);
    await first.closed();
    const secondProcess = await startOwnedChild(run);
    const secondRuntime = remote(secondProcess);
    const restored = await call(secondRuntime, "/inspect");
    const orphan = restored.audit.findLast(
      (event) => event.phase === "constructor-before-retirement",
    );
    assert.ok(
      orphan,
      "Real persisted rows must be inspected before production constructor retirement",
    );
    assert.equal(orphan.connections, 1);
    assert.equal(orphan.ready, 1);
    assert.equal(orphan.sessions, 1);
    assert.equal(orphan.attached, 1);
    assert.equal(restored.ready, 0, "A cold generation cannot resurrect socket authority");
    assert.equal(restored.sessions, 0);
    assert.equal(restored.challenges, 0);
    assert.equal(
      restored.identities,
      1,
      "Genuine registration survives abrupt process replacement",
    );
    assert.equal(restored.devices, 1);
    const secondLogin = named("login-registered-active-device");
    assert.notEqual(secondLogin.nonce, named("login-first").nonce);
    assert.notEqual(secondLogin.auth_base64, named("login-first").auth_base64);
    const second = await connect(secondRuntime, secondLogin);
    await emptySync(second);
    const resumed = await call(secondRuntime, "/inspect");
    assert.equal(resumed.ready, 1);
    assert.equal(resumed.sessions, 1);
    second.socket.close();
    await second.closed();
    await eventually(
      secondRuntime,
      (state) => state.connections === 0 && state.attached === 0 && state.offline === 0,
      "Final actual connection/presence cleanup",
    );
    await stopOwnedChild(run, secondProcess);
    const proof = {
      schema: 1,
      passed: true,
      scope:
        "actual owned abrupt ordinary runtime death, persisted orphan retirement, fresh Auth and empty Sync only; no canonical-history recovery claim",
      before,
      sqlite,
      orphan,
      restored,
      resumed,
      ownedChildren: run.children.map((instance) => ({
        pid: instance.child.pid,
        generation: instance.generation,
        ended: instance.ended,
      })),
    };
    const proofPath = join(run.runDir, "proof.json");
    await writeFile(proofPath, JSON.stringify(proof, null, 2), { flag: "wx", mode: 0o600 });
    console.log(
      JSON.stringify({
        ordinaryCrashProof: {
          path: proofPath,
          sha256: createHash("sha256")
            .update(await readFile(proofPath))
            .digest("hex"),
        },
      }),
    );
    passed = true;
  } finally {
    // Every stop re-verifies the marker and exact owned child/group. A missing
    // cleanup acknowledgement remains a failure, never a successful crash proof.
    try {
      for (const instance of run.children) await stopOwnedChild(run, instance);
    } finally {
      if (!passed) console.error(`Owned lifecycle failure evidence retained at ${run.runDir}`);
    }
  }
}
