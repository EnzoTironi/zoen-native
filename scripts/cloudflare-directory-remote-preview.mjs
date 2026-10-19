// Internal child process: all Wrangler output is captured/discarded by the runner.
// Preview bearer URLs/headers remain in memory and never enter evidence files.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { isAbsolute, join, resolve } from "node:path";

let input = "";
for await (const chunk of process.stdin) input += chunk;
const job = JSON.parse(input);
// The runner executes an immutable copy of this file in its evidence directory;
// only dependency resolution uses the reviewed checkout's pinned toolchain.
assert.ok(
  typeof job.repoRoot === "string" && isAbsolute(job.repoRoot),
  "Explicit absolute pinned toolchain checkout required",
);
const root = resolve(job.repoRoot);
assert.equal(root, job.repoRoot, "Canonical pinned toolchain path required");
const require = createRequire(join(root, "infra/cloudflare/package.json"));
const { unstable_DevEnv } = require("wrangler");
const Ws = require("ws");
const fixture = JSON.parse(await readFile(job.fixture, "utf8"));
const environment = new unstable_DevEnv();
const channels = [];
const results = [];
let phase = "start-preview";
let previewStopped = false;
let failure = null;
let diagnostic = null;
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const bounded = async (promise, ms) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("bounded private probe operation failed")), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
};
const save = async () =>
  writeFile(
    join(job.output, "remote-observations.json"),
    `${JSON.stringify(
      {
        scope:
          "Exact reviewed private Directory runtime, one new synthetic cell, actual authenticated edge sockets. No public relay, provider login, SQL inspection or forced hibernation proof.",
        results,
        failure,
        diagnostic,
      },
      null,
      2,
    )}\n`,
  );
async function checkpoint(name, action) {
  phase = name;
  const detail = await action();
  results.push({ name, passed: true, ...detail });
  await save();
}
function signChallenge(bytes) {
  const result = spawnSync(job.clientBin, [], {
    input: JSON.stringify({
      op: "auth",
      home: job.secretHome,
      challenge_base64: bytes.toString("base64"),
    }),
    encoding: "utf8",
    stdio: ["pipe", "pipe", "pipe"],
    timeout: 5000,
    maxBuffer: 2 * 1024 * 1024,
  });
  assert.equal(
    result.status,
    0,
    "Native synthetic client must accept the real canonical challenge",
  );
  const value = JSON.parse(result.stdout);
  return Buffer.from(value.auth_base64, "base64");
}
try {
  const ready = new Promise((accept, reject) => {
    environment.once("reloadComplete", (event) => accept(event.proxyData));
    environment.once("error", () => reject(new Error("private preview startup failed")));
  });
  await bounded(
    environment.startWorker({
      config: job.bridgeConfig,
      dev: {
        remote: true,
        server: { hostname: "127.0.0.1", port: 0 },
        logLevel: "none",
        watch: false,
        inspector: false,
        persist: false,
      },
    }),
    45_000,
  );
  const proxy = await bounded(ready, 45_000);
  assert.equal(proxy.userWorkerUrl.protocol, "https:");
  assert.ok(proxy.headers["cf-workers-preview-token"]);
  const edge = new URL(`https://${proxy.userWorkerUrl.hostname}`);
  const headers = { ...proxy.headers };
  async function call(path, method = "GET") {
    return bounded(fetch(new URL(path, edge), { method, headers, redirect: "manual" }), 15_000);
  }
  async function open() {
    const url = new URL("/socket", edge);
    url.protocol = "wss:";
    const socket = new Ws(url, { headers });
    socket.binaryType = "arraybuffer";
    const channel = { socket, frames: [], cursor: 0, close: null, errors: 0, forcedCleanup: false };
    channels.push(channel);
    socket.addEventListener("message", (event) => {
      // The server lane is binary only; text is a failure, never coerced.
      channel.frames.push(typeof event.data === "string" ? null : Buffer.from(event.data));
    });
    socket.on("error", () => {
      channel.errors++;
    });
    channel.closed = new Promise((accept) =>
      socket.addEventListener("close", (event) => {
        channel.close = { code: event.code, reason: event.reason };
        accept(channel.close);
      }),
    );
    await bounded(
      new Promise((accept, reject) => {
        socket.once("open", accept);
        socket.once("error", () => reject(new Error("private edge socket failed")));
      }),
      15_000,
    );
    channel.send = (encoded) => socket.send(Buffer.from(encoded, "base64"));
    channel.next = async (ms = 5000) => {
      if (channel.cursor === channel.frames.length)
        await bounded(
          new Promise((accept, reject) => {
            const received = () => {
              socket.off("close", ended);
              accept();
            };
            const ended = () => {
              socket.off("message", received);
              reject(new Error("socket closed before expected frame"));
            };
            socket.once("message", received);
            socket.once("close", ended);
          }),
          ms,
        );
      const frame = channel.frames[channel.cursor++];
      assert.ok(Buffer.isBuffer(frame), "Native Directory sends binary frames only");
      return frame;
    };
    channel.expect = async (encoded, ms = 5000) => {
      const bytes = await channel.next(ms);
      assert.deepEqual(bytes, Buffer.from(encoded, "base64"), "Exact native expected frame bytes");
    };
    channel.expectClose = async (code, ms = 5000) => {
      const closed = await bounded(channel.closed, ms);
      assert.equal(closed.code, code);
      assert.equal(socket.readyState, Ws.CLOSED, "Actual network client must reach CLOSED");
    };
    return channel;
  }
  async function login(registered) {
    const channel = await open();
    channel.send(fixture.hello_base64);
    const challenge = await channel.next();
    const auth = signChallenge(challenge);
    channel.socket.send(auth);
    await channel.expect(registered ? fixture.ready_registered_base64 : fixture.ready_new_base64);
    return { channel, challengeHash: sha256(challenge) };
  }
  await checkpoint("private-preview-and-outer-http-refusal", async () => {
    const unauthorized = await bounded(
      fetch(new URL("/socket", edge), { redirect: "manual" }),
      15_000,
    );
    assert.ok([400, 401, 403, 404].includes(unauthorized.status));
    const routes = ["/public", "/", "/v1/sync", "/arbitrary"];
    const statuses = [];
    for (const route of routes) {
      const response = await call(route);
      statuses.push(response.status);
      await response.arrayBuffer();
      assert.equal(response.status, 404);
    }
    await unauthorized.arrayBuffer();
    const payload = await bounded(
      fetch(new URL("/deliver", edge), { method: "POST", headers, body: "refused" }),
      15_000,
    );
    assert.equal(payload.status, 404, "Preview cannot accept caller-chosen delivery input");
    await payload.arrayBuffer();
    return {
      unauthorizedStatus: unauthorized.status,
      routes,
      statuses,
      deliveryPayloadStatus: payload.status,
    };
  });
  let first;
  let stale;
  await checkpoint("two-real-challenges-and-provisional-ready", async () => {
    first = await login(false);
    stale = await login(false);
    assert.notEqual(
      first.challengeHash,
      stale.challengeHash,
      "Fresh server-owned nonce per connection",
    );
    return {
      challengeHashes: [first.challengeHash, stale.challengeHash],
      canonicalChallengesVerifiedByRust: 2,
    };
  });
  await checkpoint("first-registration-and-stale-provisional-refusal", async () => {
    first.channel.send(fixture.register_base64);
    await first.channel.expect(fixture.registered_base64);
    stale.channel.send(fixture.stale_register_base64);
    await stale.channel.expect(fixture.stale_reply_base64);
    first.channel.send(fixture.large_request_base64);
    await first.channel.expect(fixture.large_reply_base64);
    first.channel.send(fixture.ping_base64);
    await first.channel.expect(fixture.pong_base64);
    return { exactRequestId: "9007199254740993", exactNativeResponses: 4 };
  });
  await checkpoint("normal-client-disconnect", async () => {
    first.channel.socket.close(1000, "owned synthetic disconnect");
    await first.channel.expectClose(1000);
    stale.channel.socket.close(1000, "owned synthetic disconnect");
    await stale.channel.expectClose(1000);
    return { actualClosedClients: 2, durableRowInspection: false };
  });
  let active;
  await checkpoint("registered-reconnect-and-canonical-service-delivery", async () => {
    active = await login(true);
    diagnostic = { stage: "registered-ready-received" };
    const response = await call("/deliver", "POST");
    diagnostic = { stage: "delivery-http", status: response.status };
    assert.equal(response.status, 200);
    const delivered = await response.json();
    diagnostic = {
      stage: "delivery-result",
      delivered: delivered.delivered,
      refused: delivered.refused,
    };
    assert.deepEqual(delivered, { delivered: 1, refused: 0 });
    await active.channel.expect(fixture.pong_base64);
    diagnostic = null;
    return { registered: true, delivered: 1, exactNativeFrame: true };
  });
  await checkpoint("self-unlink-final-send-fence-and-revoked-reconnect", async () => {
    const before = active.channel.frames.length;
    active.channel.send(fixture.unlink_base64);
    await active.channel.expectClose(1008);
    assert.equal(
      active.channel.frames.length,
      before,
      "No stale ResDone may follow self-revocation",
    );
    const response = await call("/deliver", "POST");
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { delivered: 0, refused: 0 });
    const revoked = await open();
    revoked.send(fixture.hello_base64);
    revoked.socket.send(signChallenge(await revoked.next()));
    await revoked.expect(fixture.revoked_error_base64);
    await revoked.expectClose(1008);
    return { noStaleReply: true, delayedDeliveryCount: 0, exactRevokedError: true };
  });
  await checkpoint("real-clock-untouched-hello-idle-close", async () => {
    const untouched = await open();
    // Production owns its 10s idle deadline. The additional 5s is the unchanged
    // network-close observation window; no clock override or forced cleanup.
    await untouched.expect(fixture.expired_hello_error_base64, 15_000);
    await untouched.expectClose(1008);
    return { productionIdleMs: 10_000, closeObservationMs: 5000, actualClosed: true };
  });
  await checkpoint("real-clock-auth-idle-close", async () => {
    const waiting = await open();
    waiting.send(fixture.hello_base64);
    signChallenge(await waiting.next()); // Canonical challenge check only: send no Auth.
    await waiting.expect(fixture.expired_auth_error_base64, 15_000);
    await waiting.expectClose(1008);
    return { productionIdleMs: 10_000, closeObservationMs: 5000, actualClosed: true };
  });
} catch {
  failure = {
    phase,
    detail:
      "Private remote assertion failed; foreign exception text withheld to protect preview credentials.",
  };
  await save();
} finally {
  // Failure observations are saved BEFORE cleanup. A forced OWN client teardown
  // cannot turn a failed actual-CLOSED assertion into a pass.
  const clients = channels.map((channel) => ({
    frames: channel.frames.map((frame) => frame && { bytes: frame.length, sha256: sha256(frame) }),
    close: channel.close,
    errors: channel.errors,
    readyStateBeforeCleanup: channel.socket.readyState,
    forcedOwnClientCleanupAfterObservation: channel.socket.readyState !== Ws.CLOSED,
  }));
  await writeFile(
    join(job.output, "remote-client-observations.json"),
    `${JSON.stringify(clients, null, 2)}\n`,
  );
  for (const channel of channels)
    if (channel.socket.readyState !== Ws.CLOSED) channel.socket.terminate();
  try {
    await bounded(environment.teardown(), 15_000);
    previewStopped = true;
  } catch {
    failure ??= { phase: "preview-teardown", detail: "Owned preview teardown failed." };
  }
  await writeFile(
    join(job.output, "preview-cleanup.json"),
    `${JSON.stringify({ previewStopped }, null, 2)}\n`,
  );
  await save();
}
if (failure || !previewStopped) process.exitCode = 1;
