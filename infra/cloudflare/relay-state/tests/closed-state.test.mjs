import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const moduleRoot = fileURLToPath(new URL("../", import.meta.url));
const fixturePath = process.env.ZOEN_CLOSED_FIXTURE
  ? resolve(process.env.ZOEN_CLOSED_FIXTURE)
  : join(moduleRoot, "generated/closed-fixture.json");
const native = JSON.parse(await readFile(fixturePath, "utf8"));
assert.equal(typeof native.space_id, "string");
assert.ok(
  Array.isArray(native.steps) && native.steps.length > 0,
  "Native FDB fixtures are required",
);

function runtime(path, production = false) {
  const paths = [
    production ? "src/closed-index.mjs" : "tests/closed-fixture.mjs",
    "src/closed-space.mjs",
    "src/space-storage.mjs",
    "src/relay-publications.mjs",
    "generated/wasm/admission.js",
  ];
  return new Miniflare(
    convertV4MiniflareOptions({
      modulesRoot: moduleRoot,
      modules: [...new Set(paths)]
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
  const response = await mf.dispatchFetch(`http://closed-private.test${path}`, {
    method: "POST",
    body: JSON.stringify({ space_id: native.space_id, ...input }),
  });
  return { status: response.status, body: await response.json() };
}

async function inspect(mf, spaceId = native.space_id) {
  const response = await call(mf, "/inspect", { space_id: spaceId });
  assert.equal(response.status, 200);
  return response.body.result;
}

function publication(result) {
  return {
    kind: result.kind,
    event: result.event,
    audience: result.audience,
    joined: result.joined,
  };
}

async function applyStep(mf, step) {
  const before = await inspect(mf);
  const response = await call(mf, "/step", { step });
  if (step.expected.kind === "rejected") {
    assert.equal(response.status, 400, JSON.stringify(response.body));
    assert.equal(response.body.error.reason, step.expected.reason);
    assert.equal(response.body.error.permanent, step.expected.permanent);
    assert.deepEqual(
      await inspect(mf),
      before,
      "Rust rejection must leave every persisted table unchanged",
    );
  } else {
    assert.equal(response.status, 200, JSON.stringify(response.body));
    assert.deepEqual(
      step.op === "invite" ? response.body.result : publication(response.body.result),
      step.expected,
    );
    if (step.expected.kind === "duplicate")
      assert.deepEqual(
        await inspect(mf),
        before,
        "Replay cannot consume invites or change the projection",
      );
    if (response.body.result.invite_update) {
      const record = response.body.result.invite_update;
      const stored = (await inspect(mf)).invites.find(
        (invite) => invite.code_hash === record.code_hash,
      );
      assert.deepEqual(JSON.parse(Buffer.from(stored.bytes).toString("utf8")), record);
    }
  }
  if (step.roles_after) {
    const snapshot = (await call(mf, "/snapshot")).body.result;
    assert.deepEqual(
      Object.entries(snapshot.projection?.members ?? {}).sort(([a], [b]) => a.localeCompare(b)),
      step.roles_after,
    );
  }
  return response;
}

test("private SQL adapter matches native FDB receipts, rejection reasons, rosters and invite state across restart", async () => {
  const path = await mkdtemp(join(tmpdir(), "zoen-cf-closed-"));
  let mf = runtime(path);
  try {
    let restarted = false;
    for (const step of native.steps) {
      await applyStep(mf, step);
      if (!restarted && step.op === "invite") {
        const before = await inspect(mf);
        await mf.dispose();
        mf = runtime(path);
        assert.deepEqual(await inspect(mf), before);
        restarted = true;
      }
    }
    assert.ok(restarted, "The native fixture must exercise durable invite restart");
    assert.ok(
      native.steps.some((step) => step.op === "publish" && step.expected.kind === "duplicate"),
      "The native fixture must include a removed-author or changed-envelope replay",
    );
    const history = (await call(mf, "/read")).body.result;
    const accepted = native.steps.filter(
      (step) => step.op === "publish" && step.expected.kind === "new",
    );
    assert.deepEqual(
      history.map((event) => ({
        ...event,
        bytes: Buffer.from(Object.values(event.bytes)).toString("base64"),
      })),
      accepted.map((step) => ({
        seq: step.expected.event.seq,
        prev: step.expected.event.prev,
        hash: step.expected.event.hash,
        bytes: step.expected.event.bytes_base64,
      })),
    );
    assert.equal(
      native.unsupported?.length,
      4,
      "Public/Community genesis, invalid device join and invalid-signature fixtures are required",
    );
    for (const unsupported of native.unsupported) {
      const before = await inspect(mf, unsupported.space_id);
      for (const step of unsupported.steps) {
        const result = await call(mf, "/step", { space_id: unsupported.space_id, step });
        assert.equal(result.status, 400, JSON.stringify(result.body));
        assert.equal(result.body.error.code, unsupported.expected_projection_error);
        assert.equal(
          result.body.error.permanent,
          unsupported.expected_projection_permanent ?? false,
        );
        if (unsupported.expected_error?.reason)
          assert.equal(result.body.error.reason, unsupported.expected_error.reason);
        assert.deepEqual(await inspect(mf, unsupported.space_id), before);
      }
    }
  } finally {
    await mf.dispose();
    await rm(path, { recursive: true, force: true });
  }
});

test("racing signed retries produce one exact native receipt and one projection transition", async () => {
  const mf = runtime(false);
  try {
    const genesis = native.steps.find(
      (step) => step.op === "publish" && step.expected.kind === "new",
    );
    assert.equal(genesis.expected.event.seq, "0");
    const results = await Promise.all([
      call(mf, "/step", { step: genesis }),
      call(mf, "/step", { step: genesis }),
    ]);
    for (const result of results) assert.equal(result.status, 200, JSON.stringify(result.body));
    assert.deepEqual(results.map((result) => result.body.result.kind).sort(), ["duplicate", "new"]);
    for (const result of results) {
      assert.deepEqual(result.body.result.event, genesis.expected.event);
      assert.deepEqual(result.body.result.audience, genesis.expected.audience);
      assert.equal(result.body.result.joined, genesis.expected.joined);
    }
    const state = await inspect(mf);
    assert.equal(state.entries.length, 1);
    assert.equal(state.state.length, 1);
    assert.equal(state.binding.length, 1);
  } finally {
    await mf.dispose();
  }
});

test("failed invite update rolls back the admitted event and projection before the same frame retries", async () => {
  const mf = runtime(false);
  try {
    const mint = native.steps.findIndex((step) => step.op === "invite");
    assert.ok(mint >= 0);
    const join = native.steps.findIndex(
      (step, index) =>
        index > mint &&
        step.op === "publish" &&
        step.expected.kind === "new" &&
        step.expected.joined !== null,
    );
    assert.ok(join > mint, "The native fixture must contain an accepted invite join");
    for (const step of native.steps.slice(0, join)) await applyStep(mf, step);
    const before = await inspect(mf);
    assert.equal((await call(mf, "/reject-invite-writes")).status, 200);
    const rejected = await call(mf, "/step", { step: native.steps[join] });
    assert.equal(rejected.status, 400);
    assert.match(rejected.body.error.reason, /test-only invite write failure/);
    assert.deepEqual(await inspect(mf), before);
    assert.equal((await call(mf, "/allow-invite-writes")).status, 200);
    await applyStep(mf, native.steps[join]);
    assert.equal((await inspect(mf)).entries.length, before.entries.length + 1);
  } finally {
    await mf.dispose();
  }
});

test("rejected genesis, mismatched directory lookup and absent raw append cannot mutate private tables", async () => {
  const mf = runtime(false);
  try {
    const empty = await inspect(mf);
    assert.deepEqual(empty, { binding: [], entries: [], state: [], invites: [] });
    const rejected = native.unsupported.find((fixture) => fixture.space_id !== native.space_id);
    assert.ok(rejected, "A signed unsupported genesis fixture is required");
    assert.equal(
      (await call(mf, "/step", { space_id: rejected.space_id, step: rejected.steps[0] })).status,
      400,
    );
    assert.deepEqual(await inspect(mf, rejected.space_id), empty);
    const raw = await call(mf, "/raw-append");
    assert.equal(raw.status, 400);
    assert.match(raw.body.error.reason, /appendAdmitted|not a function|not implemented/);
    const rawPublish = await call(mf, "/raw-publish");
    assert.equal(rawPublish.status, 400);
    assert.match(rawPublish.body.error.reason, /publish|not a function|not implemented/);
    assert.deepEqual(await inspect(mf), empty);

    const firstTarget = native.steps.findIndex(
      (step) =>
        step.op === "publish" && step.target_registered !== null && step.expected.kind === "new",
    );
    assert.ok(firstTarget >= 0, "A directory-backed native publication is required");
    for (const step of native.steps.slice(0, firstTarget)) await applyStep(mf, step);
    const before = await inspect(mf);
    const wrong = await call(mf, "/step", {
      step: native.steps[firstTarget],
      target_override: { identity: "f".repeat(64), registered: true },
    });
    assert.equal(wrong.status, 400);
    assert.equal(wrong.body.error.permanent, false);
    assert.deepEqual(await inspect(mf), before);
  } finally {
    await mf.dispose();
  }
});

test("closed production HTTP surface is always 404 in its own namespace", async () => {
  const mf = runtime(false, true);
  try {
    for (const path of [
      "/",
      "/healthz",
      "/readyz",
      "/v1/sync",
      "/v1/auth/challenge",
      "/append",
      "/publish",
      "/invite",
    ]) {
      assert.equal(
        (
          await mf.dispatchFetch(`http://closed-private.test${path}`, {
            method: "POST",
            body: "{}",
          })
        ).status,
        404,
      );
    }
    const config = JSON.parse(await readFile(join(moduleRoot, "wrangler.closed.jsonc"), "utf8"));
    assert.equal(config.workers_dev, false);
    assert.equal(config.preview_urls, false);
    assert.deepEqual(config.routes, []);
    assert.deepEqual(config.durable_objects.bindings, [
      { name: "CLOSED_SPACE", class_name: "ClosedSpace" },
    ]);
    assert.deepEqual(config.migrations, [{ tag: "v1", new_sqlite_classes: ["ClosedSpace"] }]);
  } finally {
    await mf.dispose();
  }
});

async function fullHistory(mf, spaceId) {
  const events = [];
  let from = "0";
  for (;;) {
    const page = await call(mf, "/read", { space_id: spaceId, from });
    assert.equal(page.status, 200, JSON.stringify(page.body));
    for (const event of page.body.result)
      events.push({ ...event, bytes: Buffer.from(Object.values(event.bytes)).toString("base64") });
    if (page.body.result.length < 256) break;
    from = (BigInt(events.at(-1).seq) + 1n).toString();
  }
  return events;
}
async function encryptedStep(mf, fixture, step) {
  const response = await call(mf, "/step", { space_id: fixture.space_id, step });
  if (step.expected.kind === "rejected") {
    assert.equal(response.status, 400, `${step.label}: ${JSON.stringify(response.body)}`);
    assert.equal(response.body.error.reason, step.expected.reason, step.label);
    assert.equal(response.body.error.permanent, step.expected.permanent, step.label);
  } else {
    assert.equal(response.status, 200, `${step.label}: ${JSON.stringify(response.body)}`);
    assert.deepEqual(publication(response.body.result), step.expected, step.label);
  }
  const snapshot = await call(mf, "/snapshot", { space_id: fixture.space_id });
  assert.equal(snapshot.status, 200, JSON.stringify(snapshot.body));
  const state = snapshot.body.result.projection;
  assert.deepEqual(
    { mls: state.mls ?? null, holds: state.holds ?? [], pruned: state.pruned ?? "0" },
    step.tracking_after,
    step.label,
  );
  assert.deepEqual(
    Object.entries(state.members).sort(([a], [b]) => a.localeCompare(b)),
    step.roles_after,
    step.label,
  );
  if (step.history_after)
    assert.deepEqual(await fullHistory(mf, fixture.space_id), step.history_after, step.label);
}

test("real core MLS Direct and Group ciphertext matches native FDB epochs, device Welcomes, holds, pruning and exact history across restart", async () => {
  assert.equal(native.encrypted.length, 2);
  assert.ok(native.encrypted.reduce((sum, fixture) => sum + fixture.actual_peer_opens, 0) > 270);
  const path = await mkdtemp(join(tmpdir(), "zoen-cf-mls-"));
  let mf = runtime(path);
  try {
    for (const fixture of native.encrypted) {
      let restarted = false;
      for (const step of fixture.steps) {
        await encryptedStep(mf, fixture, step);
        if (!restarted && step.label === "actual epoch-zero commit") {
          const before = await inspect(mf, fixture.space_id);
          await mf.dispose();
          mf = runtime(path);
          assert.deepEqual(await inspect(mf, fixture.space_id), before);
          restarted = true;
        }
      }
      assert.ok(restarted);
      assert.deepEqual(await fullHistory(mf, fixture.space_id), fixture.history);
    }
    assert.ok(native.encrypted.some((fixture) => fixture.cold_native_epoch_checked));
  } finally {
    await mf.dispose();
    await rm(path, { recursive: true, force: true });
  }
});

test("a late ciphertext pruning failure rolls back the checkpoint, hold update and receipt before exact retry", async () => {
  const fixture = native.encrypted.find((fixture) => fixture.kind === "Direct");
  const index = fixture.steps.findIndex(
    (step) => step.label === "peer checkpoint prunes one bounded native batch",
  );
  assert.ok(index > 270);
  const mf = runtime(false);
  try {
    for (const step of fixture.steps.slice(0, index)) await encryptedStep(mf, fixture, step);
    const before = await inspect(mf, fixture.space_id);
    assert.equal(
      (await call(mf, "/reject-prune-writes", { space_id: fixture.space_id })).status,
      200,
    );
    const failed = await call(mf, "/step", {
      space_id: fixture.space_id,
      step: fixture.steps[index],
    });
    assert.equal(failed.status, 400);
    assert.match(failed.body.error.reason, /test-only pruning write failure/);
    assert.deepEqual(await inspect(mf, fixture.space_id), before);
    assert.equal(
      (await call(mf, "/allow-prune-writes", { space_id: fixture.space_id })).status,
      200,
    );
    for (const step of fixture.steps.slice(index)) await encryptedStep(mf, fixture, step);
  } finally {
    await mf.dispose();
  }
});

test("racing identical real MLS commits advance the durable epoch exactly once", async () => {
  const fixture = native.encrypted.find((fixture) => fixture.kind === "Direct");
  const index = fixture.steps.findIndex((step) => step.label === "actual epoch-zero commit");
  const mf = runtime(false);
  try {
    for (const step of fixture.steps.slice(0, index)) await encryptedStep(mf, fixture, step);
    const step = fixture.steps[index];
    const results = await Promise.all([
      call(mf, "/step", { space_id: fixture.space_id, step }),
      call(mf, "/step", { space_id: fixture.space_id, step }),
    ]);
    assert.deepEqual(results.map((result) => result.body.result.kind).sort(), ["duplicate", "new"]);
    for (const result of results) {
      assert.equal(result.status, 200);
      assert.deepEqual(result.body.result.event, step.expected.event);
    }
    const state = (await call(mf, "/snapshot", { space_id: fixture.space_id })).body.result
      .projection;
    assert.deepEqual(state.mls, step.tracking_after.mls);
    assert.deepEqual(state.holds, step.tracking_after.holds);
    assert.equal((await inspect(mf, fixture.space_id)).entries.length, index + 1);
  } finally {
    await mf.dispose();
  }
});
