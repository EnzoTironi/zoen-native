import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const moduleRoot = fileURLToPath(new URL("../", import.meta.url));
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_CLOSED_FIXTURE
      ? resolve(process.env.ZOEN_CLOSED_FIXTURE)
      : join(moduleRoot, "generated/closed-fixture.json"),
    "utf8",
  ),
);
assert.equal(
  native.authorized_reads?.length,
  1,
  "Actual native FDB/core MLS read oracle is required",
);
const fixture = native.authorized_reads[0];
assert.equal(fixture.actual_peer_opens, 65);
assert.ok(fixture.byte_limited_pages_before_removal > 1);

function runtime(path) {
  return new Miniflare(
    convertV4MiniflareOptions({
      modulesRoot: moduleRoot,
      modules: [
        "tests/closed-fixture.mjs",
        "src/closed-space.mjs",
        "src/space-storage.mjs",
        "src/relay-publications.mjs",
        "generated/wasm/admission.js",
      ]
        .map((path) => ({ type: "ESModule", path: join(moduleRoot, path) }))
        .concat({
          type: "CompiledWasm",
          path: join(moduleRoot, "generated/wasm/admission_bg.wasm"),
        }),
      compatibilityDate: "2026-10-18",
      durableObjects: { CLOSED_SPACE: { className: "ClosedSpace", useSQLite: true } },
      ...(path ? { resourcePersistencePath: path } : {}),
    }),
  );
}
async function call(mf, path, input = {}) {
  const response = await mf.dispatchFetch(`https://private-read.test${path}`, {
    method: "POST",
    body: JSON.stringify({ space_id: fixture.space_id, ...input }),
  });
  return { status: response.status, body: await response.json() };
}
async function publish(mf, step) {
  const response = await call(mf, "/step", { step });
  assert.equal(response.status, 200, `${step.label}: ${JSON.stringify(response.body)}`);
  const { kind, event, audience, joined } = response.body.result;
  assert.deepEqual({ kind, event, audience, joined }, step.expected, step.label);
}

test("actual native removal/rejoin visibility and byte-limited MLS pages agree through persisted restarts", async () => {
  const path = await mkdtemp(join(tmpdir(), "zoen-cf-authorized-read-"));
  let mf = runtime(path);
  let restarts = 0,
    byteLimited = 0;
  let currentNativeMembers = [];
  try {
    for (const step of fixture.steps) {
      if (step.op === "read_for") {
        const response = await call(mf, "/read-authorized", {
          session: step.session,
          from: step.from,
          limit: step.limit,
        });
        assert.equal(response.status, 200, `${step.label}: ${JSON.stringify(response.body)}`);
        assert.deepEqual(response.body.result, step.expected, step.label);
        const { events, done, has_more, next_seq } = response.body.result;
        assert.equal(done, !has_more);
        assert.equal(next_seq === null, done);
        const bytes = events.reduce(
          (sum, event) => sum + Buffer.from(event.bytes, "base64").byteLength,
          0,
        );
        assert.equal(bytes, step.expected_bytes);
        assert.ok(bytes <= 4 * 1024 * 1024);
        if (has_more && events.length < step.limit) byteLimited++;
        const visibility = await call(mf, "/visibility", { session: step.session });
        assert.equal(visibility.status, 200);
        assert.deepEqual(visibility.body.result, step.expected.visibility);
        const peers = await call(mf, "/co-members", {
          identity: step.session.authenticated_identity,
        });
        assert.equal(peers.status, 200);
        assert.ok(Array.isArray(peers.body.result.members));
        assert.ok(!peers.body.result.members.includes(step.session.authenticated_identity));
        const expectedPeers = currentNativeMembers.includes(step.session.authenticated_identity)
          ? currentNativeMembers.filter((id) => id !== step.session.authenticated_identity).sort()
          : [];
        assert.deepEqual(peers.body.result.members, expectedPeers, step.label);
        if (step.label === "unknown Space cannot grant or bind a read") {
          assert.deepEqual((await call(mf, "/inspect")).body.result, {
            binding: [],
            entries: [],
            state: [],
            invites: [],
          });
        }
      } else {
        await publish(mf, step);
        currentNativeMembers = step.roles_after.map(([identity]) => identity);
        if (
          [
            "read member removal records boundary",
            "read same identity rejoins with a fresh device",
          ].includes(step.label)
        ) {
          const before = (await call(mf, "/snapshot")).body.result;
          await mf.dispose();
          mf = runtime(path);
          assert.deepEqual((await call(mf, "/snapshot")).body.result, before);
          restarts++;
        }
      }
    }
    assert.equal(restarts, 2);
    assert.ok(
      byteLimited >= 2,
      "Actual ciphertext must exercise byte-cap continuation before and after rejoin",
    );
  } finally {
    await mf.dispose();
    await rm(path, { recursive: true, force: true });
  }
});

test("authorized read input cannot assert membership or mutate an unknown Space", async () => {
  const mf = runtime(false);
  try {
    const step = fixture.steps.find((step) => step.op === "read_for");
    assert.ok(step);
    const expectedEmpty = { binding: [], entries: [], state: [], invites: [] };
    for (const input of [
      { session: { ...step.session, authenticated_device: "not-a-device" }, from: "0", limit: 256 },
      { session: step.session, from: "00", limit: 256 },
      { session: step.session, from: "18446744073709551616", limit: 256 },
      { session: step.session, from: "0", limit: 0 },
      { session: step.session, from: "0", limit: 257 },
      { session: step.session, from: "0", limit: 1.5 },
    ]) {
      assert.equal((await call(mf, "/read-authorized", input)).status, 400);
      assert.deepEqual((await call(mf, "/inspect")).body.result, expectedEmpty);
    }
    const peers = await call(mf, "/co-members", {
      identity: step.session.authenticated_identity,
      member: true,
    });
    assert.equal(peers.status, 200);
    assert.deepEqual(peers.body.result, { members: [] });
    assert.equal((await call(mf, "/co-members", { identity: "bad-identity" })).status, 400);
    assert.deepEqual((await call(mf, "/inspect")).body.result, expectedEmpty);
    const result = await call(mf, "/read-authorized", {
      session: step.session,
      from: "0",
      limit: 256,
      member: true,
      removed_at: "1000",
    });
    assert.equal(result.status, 200);
    assert.deepEqual(result.body.result.events, []);
    assert.equal(result.body.result.done, true);
    assert.deepEqual(result.body.result.visibility, { member: false, removed_at: null });
    assert.deepEqual((await call(mf, "/inspect")).body.result, expectedEmpty);
  } finally {
    await mf.dispose();
  }
});
