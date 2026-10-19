import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { initSync, directorySealedProfileReply } from "../generated/wasm/admission.js";

const root = fileURLToPath(new URL("../", import.meta.url));
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_DIRECTORY_FIXTURE
      ? resolve(process.env.ZOEN_DIRECTORY_FIXTURE)
      : join(root, "generated/directory-fixture.json"),
    "utf8",
  ),
);
assert.equal(native.schema, 1);
assert.equal(native.steps.length, 71);
const timeline = native.sealed_profiles;
assert.ok(
  timeline.steps.length >= 30,
  "Require actual native signed-profile and agreement SQL vectors",
);
for (const property of ["roundtrip", "wrong_version_rejected", "wrong_key_rejected"])
  assert.equal(
    timeline.crypto_proof[property],
    true,
    `Require actual native AEAD proof: ${property}`,
  );
assert.match(timeline.crypto_proof.ciphertext_sha256, /^[a-f0-9]{64}$/);
initSync({
  module: new WebAssembly.Module(await readFile(join(root, "generated/wasm/admission_bg.wasm"))),
});

function runtime(path) {
  return new Miniflare(
    convertV4MiniflareOptions({
      modulesRoot: root,
      modules: [
        "tests/directory-fixture.mjs",
        "src/directory-state.mjs",
        "src/provider-auth.mjs",
        "src/media-blobs.mjs",
        "src/blob-io.mjs",
        "src/blob-quota.mjs",
        "src/transfer-blobs.mjs",
        "generated/wasm/admission.js",
      ]
        .map((path) => ({ type: "ESModule", path: join(root, path) }))
        .concat({ type: "CompiledWasm", path: join(root, "generated/wasm/admission_bg.wasm") }),
      compatibilityDate: "2026-10-18",
      bindings: { RELAY_NAME: "zoen-cloudflare-private", DIRECTORY_CELL: "private-native-fixture" },
      durableObjects: { DIRECTORY: { className: "DirectoryState", useSQLite: true } },
      ...(path ? { resourcePersistencePath: path } : {}),
    }),
  );
}
async function call(mf, path, input = {}) {
  const response = await mf.dispatchFetch(`http://directory.test${path}`, {
    method: "POST",
    body: JSON.stringify(input),
  });
  return { status: response.status, body: await response.json() };
}
const encryptedState = async (mf) => (await call(mf, "/encrypted-state")).body.result;
const accountState = async (mf) => (await call(mf, "/inspect")).body.result;
function stepFor(tag) {
  const step = timeline.steps.find((step) => step.encrypted_test_case === tag);
  assert.ok(step, `Missing native encrypted case ${tag}`);
  return step;
}
async function apply(mf, step) {
  const before = await encryptedState(mf);
  const result = await call(mf, "/step", { ...step, enforce_single_clock: true });
  if (step.expected.kind === "rejected") {
    assert.equal(result.status, 400, step.label);
    assert.equal(result.body.error.reason, step.expected.reason, step.label);
    assert.deepEqual(
      await encryptedState(mf),
      before,
      "Refused writes must retain exact ciphertext and proof",
    );
  } else {
    assert.equal(result.status, 200, JSON.stringify({ label: step.label, ...result.body }));
    if (step.unordered_encrypted_rows) {
      const normalize = (encoded) => {
        const decoded = JSON.parse(directorySealedProfileReply(Buffer.from(encoded, "base64")));
        decoded.rows.sort((a, b) => a.identity.localeCompare(b.identity));
        return decoded;
      };
      assert.deepEqual(
        normalize(result.body.result.reply_base64),
        normalize(step.expected.reply_base64),
        step.label,
      );
    } else assert.deepEqual(result.body.result, step.expected, step.label);
  }
  assert.deepEqual(await encryptedState(mf), step.encrypted_state_after, step.label);
  return result;
}
async function before(mf, selected) {
  const end = timeline.steps.indexOf(selected);
  assert.ok(end >= 0);
  for (const step of timeline.steps.slice(0, end)) await apply(mf, step);
}

test("signed encrypted profiles and agreement keys match actual native SQL and survive restart with exact BIGINT versions", async () => {
  const path = await mkdtemp(join(tmpdir(), "zoen-directory-sealed-"));
  let mf = runtime(path);
  let restarted = false;
  try {
    for (const step of timeline.steps) {
      await apply(mf, step);
      if (!restarted && step.encrypted_test_case === "profile-first") {
        const saved = await encryptedState(mf);
        assert.ok(
          saved.profiles.some((row) => BigInt(row.version) > BigInt(Number.MAX_SAFE_INTEGER)),
          "Native fixture must cross the JS Number precision boundary",
        );
        await mf.dispose();
        mf = runtime(path);
        assert.deepEqual(await encryptedState(mf), saved);
        restarted = true;
      }
    }
    assert.equal(restarted, true);
  } finally {
    await mf.dispose();
    await rm(path, { recursive: true, force: true });
  }
});

for (const [table, tag] of [
  ["profile", "profile-first"],
  ["agreement", "agreement-first"],
]) {
  test(`a late ${table} SQL failure rolls back ciphertext and proof before exact retry`, async () => {
    const mf = runtime();
    try {
      const selected = stepFor(tag);
      await before(mf, selected);
      const encrypted = await encryptedState(mf);
      const accounts = await accountState(mf);
      assert.equal((await call(mf, "/reject", { table })).status, 200);
      assert.equal((await call(mf, "/step", selected)).status, 400);
      assert.deepEqual(await encryptedState(mf), encrypted);
      assert.deepEqual(await accountState(mf), accounts);
      assert.equal((await call(mf, "/allow")).status, 200);
      await apply(mf, selected);
    } finally {
      await mf.dispose();
    }
  });
}

test("competing profile versions leave the newest ciphertext and retain native stale-write refusal", async () => {
  const mf = runtime();
  try {
    const first = stepFor("profile-first");
    const newer = stepFor("profile-newer");
    await before(mf, first);
    const results = await Promise.all([call(mf, "/step", first), call(mf, "/step", newer)]);
    assert.equal(results[1].status, 200, JSON.stringify(results));
    if (results[0].status !== 200) {
      assert.equal(results[0].status, 400);
      assert.equal(results[0].body.error.reason, "stale profile version");
    }
    const latest = newer.encrypted_state_after.profiles.find((row) =>
      first.encrypted_state_after.profiles.some((old) => old.identity === row.identity),
    );
    assert.ok(latest);
    assert.deepEqual(
      (await encryptedState(mf)).profiles.find((row) => row.identity === latest.identity),
      latest,
    );
    const changed = stepFor("profile-same-version-different");
    const refused = await call(mf, "/step", changed);
    assert.equal(refused.status, 400);
    assert.equal(refused.body.error.reason, "stale profile version");
    assert.deepEqual(
      (await encryptedState(mf)).profiles.find((row) => row.identity === latest.identity),
      latest,
    );
  } finally {
    await mf.dispose();
  }
});

test("a revoked linked device cannot replace profile ciphertext after persisted runtime restart", async () => {
  const path = await mkdtemp(join(tmpdir(), "zoen-directory-sealed-revoked-"));
  let mf = runtime(path);
  try {
    const unlink = stepFor("unlink-linked");
    await before(mf, unlink);
    await apply(mf, unlink);
    const state = await encryptedState(mf);
    await mf.dispose();
    mf = runtime(path);
    const refused = await call(mf, "/step", stepFor("profile-linked"));
    assert.equal(refused.status, 400);
    assert.equal(refused.body.error.reason, "this device was unlinked");
    assert.deepEqual(await encryptedState(mf), state);
  } finally {
    await mf.dispose();
    await rm(path, { recursive: true, force: true });
  }
});
