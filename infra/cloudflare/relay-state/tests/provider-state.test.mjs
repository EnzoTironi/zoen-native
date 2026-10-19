import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const root = fileURLToPath(new URL("../", import.meta.url));
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_PROVIDER_FIXTURE
      ? resolve(process.env.ZOEN_PROVIDER_FIXTURE)
      : join(root, "generated/provider-enrollment-fixtures.json"),
    "utf8",
  ),
);
assert.equal(native.schema, 1);
assert.equal(native.scenarios.length, 20, "Every actual native SQL scenario is required");
const named = (label) => {
  const scenario = native.scenarios.find((value) => value.label === label);
  assert.ok(scenario, `Required actual native scenario: ${label}`);
  return scenario;
};
const issue = native.scenarios.find(
  (value) => value.steps[0].op === "issue" && value.steps[0].expected.status === 200,
);
assert.ok(issue);
function runtime() {
  return new Miniflare(
    convertV4MiniflareOptions({
      modulesRoot: root,
      modules: [
        "tests/provider-fixture.mjs",
        "src/directory-state.mjs",
        "src/provider-auth.mjs",
        "src/media-blobs.mjs",
        "generated/wasm/admission.js",
      ]
        .map((path) => ({ type: "ESModule", path: join(root, path) }))
        .concat({ type: "CompiledWasm", path: join(root, "generated/wasm/admission_bg.wasm") }),
      compatibilityDate: "2026-10-18",
      bindings: { RELAY_NAME: "zoen-cloudflare-private", DIRECTORY_CELL: "provider-fixture" },
      durableObjects: { DIRECTORY: { className: "DirectoryState", useSQLite: true } },
    }),
  );
}
async function fixture(mf, action, body = {}, name = "provider-fixture") {
  const response = await mf.dispatchFetch(
    `https://private.fixture/fixture/${action}?name=${name}`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
  assert.equal(response.status, 200);
  return response.json();
}
async function configure(mf, options = {}, name) {
  return fixture(
    mf,
    "configure",
    { settings: native.settings, jwks: native.jwks, now_ms: issue.steps[0].now_ms, ...options },
    name,
  );
}
async function request(mf, kind, body, origin, name = "provider-fixture", raw) {
  const response = await mf.dispatchFetch(`https://private.fixture/v1/auth/${kind}?name=${name}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...(origin === null ? {} : { Origin: origin }) },
    body: raw ?? JSON.stringify(body),
  });
  return { status: response.status, body: await response.json(), headers: response.headers };
}
async function setup(mf, scenario, options = {}, name) {
  await configure(mf, options, name);
  await fixture(mf, "seed", scenario.initial_state, name);
}
async function apply(mf, step, name) {
  await fixture(mf, "clock", step, name);
  const response = await request(
    mf,
    step.op === "issue" ? "verify" : "bind",
    step.body,
    step.origin,
    name,
  );
  assert.equal(response.status, step.expected.status, step.label);
  assert.deepEqual(response.body, step.expected.body, step.label);
  assert.equal(response.headers.get("Cache-Control"), "no-store");
  assert.equal(response.headers.get("Pragma"), "no-cache");
  assert.equal(response.headers.get("X-Content-Type-Options"), "nosniff");
  const stored = await fixture(mf, "inspect", {}, name);
  assert.deepEqual(stored, step.state_after, step.label);
  if (step.op === "issue" && response.status === 200) {
    assert.ok(
      !JSON.stringify(stored).includes(response.body.proof_token),
      "Only the Rust proof hash is durable",
    );
    assert.ok(
      !JSON.stringify(stored).includes(step.body.id_token),
      "Provider assertions are never durable",
    );
  }
  return response;
}

for (const scenario of native.scenarios) {
  test(`private provider host matches actual native SQL: ${scenario.label}`, async () => {
    const mf = runtime();
    try {
      await setup(mf, scenario);
      for (const step of scenario.steps) await apply(mf, step);
    } finally {
      await mf.dispose();
    }
  });
}

test("real Request Origin controls challenge policy; body origin or subject cannot become trusted facts", async () => {
  const mf = runtime();
  try {
    await configure(mf, { entropy: ["11".repeat(32), "aa".repeat(32), "bb".repeat(32)] });
    const body = { provider: "google", client: "web" };
    assert.deepEqual((await request(mf, "challenge", body, null)).body, {
      error: "origin_required",
    });
    assert.deepEqual((await request(mf, "challenge", body, "https://untrusted.example")).body, {
      error: "origin_refused",
    });
    assert.equal(
      (
        await request(
          mf,
          "challenge",
          { ...body, origin: native.settings.origins[0] },
          "https://untrusted.example",
        )
      ).status,
      400,
    );
    const response = await request(mf, "challenge", body, native.settings.origins[0]);
    assert.equal(response.status, 200);
    assert.equal(response.body.id, "11".repeat(32));
    assert.equal(response.headers.get("Access-Control-Allow-Origin"), native.settings.origins[0]);
    assert.equal((await fixture(mf, "inspect")).challenges[0].origin, native.settings.origins[0]);
  } finally {
    await mf.dispose();
  }
});

for (const [label, response] of [
  ["redirect", { status: 302, headers: { Location: "https://untrusted.example/keys" } }],
  ["declared oversized JWKS", { headers: { "Content-Length": "65537" } }],
  ["streamed oversized JWKS", { body: "x".repeat(65537) }],
]) {
  test(`JWKS ${label} refuses before proof consumption and only uses the fixed provider URL`, async () => {
    const mf = runtime();
    try {
      await setup(mf, issue, { response });
      const step = issue.steps[0];
      await fixture(mf, "clock", step);
      const result = await request(mf, "verify", step.body, step.origin);
      assert.deepEqual(
        { status: result.status, body: result.body },
        { status: 401, body: { error: "invalid_provider_assertion" } },
      );
      assert.deepEqual(await fixture(mf, "inspect"), issue.initial_state);
      assert.deepEqual(await fixture(mf, "fetches"), [
        { url: "https://www.googleapis.com/oauth2/v3/certs", redirect: "manual" },
      ]);
    } finally {
      await mf.dispose();
    }
  });
}

test("JWKS timeout aborts at the native five-second boundary and does not consume the challenge", async () => {
  const mf = runtime();
  try {
    await setup(mf, issue, { response: { waitForAbort: true } });
    const step = issue.steps[0];
    await fixture(mf, "clock", step);
    const start = performance.now();
    const result = await request(mf, "verify", step.body, step.origin);
    assert.ok(performance.now() - start >= 4500, "The actual five-second host timer must run");
    assert.deepEqual(
      { status: result.status, body: result.body },
      { status: 401, body: { error: "invalid_provider_assertion" } },
    );
    assert.deepEqual(await fixture(mf, "inspect"), issue.initial_state);
  } finally {
    await mf.dispose();
  }
});

test("failed JWKS refresh is throttled for ten seconds, then the same challenge can succeed", async () => {
  const mf = runtime();
  try {
    await setup(mf, issue, { response: { body: '{"keys":[]}' } });
    const step = issue.steps[0];
    for (let attempt = 0; attempt < 2; attempt++) {
      await fixture(mf, "clock", step);
      assert.equal((await request(mf, "verify", step.body, step.origin)).status, 401);
    }
    assert.equal((await fixture(mf, "fetches")).length, 1);
    await configure(mf, { cache_now_ms: 10_000 });
    await apply(mf, { ...step, cache_now_ms: 10_000 });
    assert.equal((await fixture(mf, "fetches")).length, 2);
  } finally {
    await mf.dispose();
  }
});

for (const [label, cacheControl, ttl] of [
  [
    "overflow is skipped before a leading-plus directive",
    "max-age=18446744073709551616, max-age=+120",
    120,
  ],
  ["overflow alone uses the native default", "max-age=18446744073709551616", 300],
  ["zero is clamped to the native minimum", "max-age=0", 60],
  [
    "valid u64 maximum is clamped without floating-point overflow",
    "max-age=18446744073709551615",
    3600,
  ],
]) {
  test(`JWKS cache ${label}`, async () => {
    const mf = runtime();
    try {
      const original = issue.steps[0];
      const challenge = issue.initial_state.challenges.find(
        (value) => value.id === original.body.challenge_id,
      );
      assert.ok(challenge);
      const fresh = async (index, cacheNow) => {
        const id = String(10 + index).repeat(32),
          proof = String(30 + index).repeat(32);
        await configure(mf, {
          cache_now_ms: cacheNow,
          response: { cacheControl },
          entropy: [id, challenge.nonce, challenge.state, proof],
        });
        const issued = await request(
          mf,
          "challenge",
          { provider: "google", client: "web" },
          original.origin,
        );
        assert.equal(issued.status, 200);
        const verified = await request(
          mf,
          "verify",
          { ...original.body, challenge_id: id },
          original.origin,
        );
        assert.equal(verified.status, 200);
        assert.equal(verified.body.proof_token, proof);
      };
      await fresh(1, 0);
      assert.equal((await fixture(mf, "fetches")).length, 1);
      await fresh(2, ttl * 1000 - 1);
      assert.equal((await fixture(mf, "fetches")).length, 1);
      await fresh(3, ttl * 1000);
      assert.equal((await fixture(mf, "fetches")).length, 2);
    } finally {
      await mf.dispose();
    }
  });
}

test("concurrent verification consumes one challenge and issues exactly one proof", async () => {
  const mf = runtime();
  try {
    await setup(mf, issue);
    const step = issue.steps[0];
    const other = "ef".repeat(32);
    assert.notEqual(step.entropy[0], other);
    await fixture(mf, "clock", { ...step, entropy: [...step.entropy, other] });
    const results = await Promise.all([
      request(mf, "verify", step.body, step.origin),
      request(mf, "verify", step.body, step.origin),
    ]);
    assert.deepEqual(results.map((value) => value.status).sort(), [200, 401]);
    assert.deepEqual(results.find((value) => value.status === 401).body, {
      error: "challenge_expired",
    });
    const state = await fixture(mf, "inspect");
    assert.equal(state.proofs.length, 1);
    assert.notEqual(state.challenges[0].consumed_ms, null);
  } finally {
    await mf.dispose();
  }
});

test("late proof INSERT and bind receipt UPDATE failures roll back their entire SQL transactions", async () => {
  for (const [scenario, kind] of [
    [issue, "proof-insert"],
    [named("bind-first"), "proof-update"],
  ]) {
    const mf = runtime();
    try {
      await setup(mf, scenario);
      const step = scenario.steps[0];
      await fixture(mf, "clock", step);
      await fixture(mf, "reject", { kind });
      const result = await request(
        mf,
        step.op === "issue" ? "verify" : "bind",
        step.body,
        step.origin,
      );
      assert.deepEqual(
        { status: result.status, body: result.body },
        { status: 503, body: { error: "authentication_unavailable" } },
      );
      assert.deepEqual(await fixture(mf, "inspect"), scenario.initial_state);
      await fixture(mf, "allow");
      await apply(mf, step);
    } finally {
      await mf.dispose();
    }
  }
});

test("concurrent identical proof binds produce one identity/device/binding and the original durable response", async () => {
  const mf = runtime();
  try {
    const scenario = named("bind-first");
    await setup(mf, scenario);
    const step = scenario.steps[0];
    await fixture(mf, "clock", step);
    const results = await Promise.all([
      request(mf, "bind", step.body, step.origin),
      request(mf, "bind", step.body, step.origin),
    ]);
    for (const result of results)
      assert.deepEqual({ status: result.status, body: result.body }, step.expected);
    assert.deepEqual(await fixture(mf, "inspect"), step.state_after);
  } finally {
    await mf.dispose();
  }
});

test("the request body limit counts actual bytes before any provider mutation", async () => {
  const mf = runtime();
  try {
    await setup(mf, issue);
    const step = issue.steps[0];
    await fixture(mf, "clock", step);
    const raw = " ".repeat(32768) + JSON.stringify(step.body);
    const result = await request(mf, "verify", step.body, step.origin, undefined, raw);
    assert.deepEqual(
      { status: result.status, body: result.body },
      { status: 400, body: { error: "invalid_authentication" } },
    );
    assert.deepEqual(await fixture(mf, "inspect"), issue.initial_state);
    assert.deepEqual(await fixture(mf, "fetches"), []);
  } finally {
    await mf.dispose();
  }
});

test("raw duplicate JSON fields are rejected by Rust before challenge or bind writes", async () => {
  const mf = runtime();
  try {
    await configure(mf);
    const before = await fixture(mf, "inspect");
    const duplicate = '{"provider":"google","provider":"google","client":"web"}';
    const challenge = await request(
      mf,
      "challenge",
      null,
      native.settings.origins[0],
      undefined,
      duplicate,
    );
    assert.deepEqual(
      { status: challenge.status, body: challenge.body },
      { status: 400, body: { error: "invalid_authentication" } },
    );
    assert.deepEqual(await fixture(mf, "inspect"), before);
  } finally {
    await mf.dispose();
  }
  const bound = runtime();
  try {
    const scenario = named("bind-first");
    await setup(bound, scenario);
    const step = scenario.steps[0];
    await fixture(bound, "clock", step);
    const duplicate = `{"proof_token":${JSON.stringify(step.body.proof_token)},${JSON.stringify(step.body).slice(1)}`;
    const result = await request(bound, "bind", null, step.origin, undefined, duplicate);
    assert.deepEqual(
      { status: result.status, body: result.body },
      { status: 400, body: { error: "invalid_authentication" } },
    );
    assert.deepEqual(await fixture(bound, "inspect"), scenario.initial_state);
  } finally {
    await bound.dispose();
  }
});
