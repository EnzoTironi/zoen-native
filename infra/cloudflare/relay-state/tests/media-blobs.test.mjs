import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const root = fileURLToPath(new URL("../", import.meta.url));
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_MEDIA_FIXTURE
      ? resolve(process.env.ZOEN_MEDIA_FIXTURE)
      : join(root, "generated/media-fixture.json"),
    "utf8",
  ),
);
assert.equal(native.schema, 1);
assert.equal(native.scope, "native-media-http");
assert.ok(native.steps.length >= 28);
const first = native.steps.find((step) => step.label === "media-first");
assert.ok(first?.body?.base64, "A real native-sealed ciphertext is required");
assert.ok(native.decryption?.key_hex && native.decryption?.plain_base64);
const body = (descriptor) =>
  descriptor?.base64 !== undefined
    ? Buffer.from(descriptor.base64, "base64")
    : descriptor?.repeat_byte !== undefined
      ? Buffer.alloc(Number(descriptor.length), descriptor.repeat_byte)
      : undefined;
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

function runtime(persistence) {
  return new Miniflare(
    convertV4MiniflareOptions({
      modulesRoot: root,
      modules: [
        "tests/media-fixture.mjs",
        "tests/directory-fixture.mjs",
        "src/directory-index.mjs",
        "src/relay-entrypoint.mjs", "src/relay-route.mjs", "src/directory-state.mjs", "src/relay-composition.mjs",
        "src/provider-auth.mjs",
        "src/media-blobs.mjs",
        "src/blob-io.mjs", "src/blob-quota.mjs", "src/transfer-blobs.mjs",
        "generated/wasm/admission.js",
      ]
        .map((path) => ({ type: "ESModule", path: join(root, path) }))
        .concat({ type: "CompiledWasm", path: join(root, "generated/wasm/admission_bg.wasm") }),
      compatibilityDate: "2026-10-18",
      bindings: { RELAY_NAME: native.relay, DIRECTORY_CELL: "private-media-fixture" },
      durableObjects: { DIRECTORY: { className: "DirectoryState", useSQLite: true } },
      r2Buckets: { MEDIA_BUCKET: "local-private-media" },
      ...(persistence ? { resourcePersistencePath: persistence } : {}),
    }),
  );
}

async function control(mf, path, input = {}, name) {
  const response = await mf.dispatchFetch(
    `https://private.fixture/${path}${name ? `?name=${name}` : ""}`,
    {
      method: "POST",
      body: JSON.stringify(input),
    },
  );
  assert.equal(response.status, 200, await response.clone().text());
  return response.json();
}

async function seed(mf, name) {
  return control(mf, "seed", { ...native, now_ms: first.now_ms }, name);
}

async function request(mf, step = first, name, prefix = "") {
  const response = await mf.dispatchFetch(
    `https://private.fixture${prefix}${step.path ?? `/v1/blobs/${step.sha}`}${name ? `?name=${name}` : ""}`,
    {
      method: step.op === "get" ? "GET" : "PUT",
      headers: step.headers,
      body: step.op === "get" ? undefined : body(step.body),
    },
  );
  return {
    status: response.status,
    body_base64: Buffer.from(await response.arrayBuffer()).toString("base64"),
    headers: response.headers,
  };
}

function sameResponse(actual, expected, label) {
  assert.equal(actual.status, expected.status, label);
  assert.equal(actual.body_base64, expected.body_base64, label);
  for (const [name, value] of Object.entries(expected.headers ?? {})) {
    if (["vary", "access-control-allow-methods", "access-control-allow-headers"].includes(name)) {
      const tokens = (value) =>
        value
          ?.toLowerCase()
          .split(",")
          .map((item) => item.trim())
          .sort();
      assert.deepEqual(tokens(actual.headers.get(name)), tokens(value), `${label}: ${name}`);
    } else assert.equal(actual.headers.get(name), value, `${label}: ${name}`);
  }
}

async function until(check, label) {
  const limit = Date.now() + 5000;
  for (;;) {
    const value = await check();
    if (value) return value;
    assert.ok(Date.now() < limit, label);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

test("private R2 media matches every actual native HTTP response, extractor bound, immutable bytes and revoked-device rule", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    for (const step of native.steps) {
      await control(mf, "configure", { now_ms: step.now_ms });
      if (step.op === "revoke") await control(mf, "unlink", step);
      else sameResponse(await request(mf, step), step.expected, step.label);
      const stored = await control(mf, "objects");
      assert.deepEqual(
        stored.map(({ metadata: _metadata, ...object }) => object),
        step.objects_after,
        step.label,
      );
      assert.ok(
        stored.every((object) => Object.keys(object.metadata ?? {}).length === 0),
        "No keys, identity or other uploader metadata",
      );
    }
    assert.ok(native.steps.some((step) => step.label === "max" && step.expected.status === 201));
    assert.ok(
      native.steps.some((step) => step.label === "max-plus-one" && step.expected.status === 413),
    );
    assert.ok(
      native.steps.some((step) => step.label === "extractor-limit" && step.expected.status === 413),
    );
  } finally {
    await mf.dispose();
  }
});

test("actual fixed-cell PrivateDirectory media delegation works while every outer HTTP route stays404", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    assert.equal((await request(mf, first, undefined, "/facade")).status, 201);
    const read = await request(mf, { op: "get", sha: first.sha }, undefined, "/facade");
    assert.equal(read.body_base64, first.body.base64);
    assert.equal(digest(Buffer.from(read.body_base64, "base64")), first.sha);
    for (const path of ["/v1/sync", `/v1/blobs/${first.sha}`, "/v1/auth/challenge", "/health"])
      assert.equal((await mf.dispatchFetch(`https://private.fixture/public${path}`)).status, 404);
    const config = JSON.parse(await readFile(join(root, "wrangler.directory.jsonc"), "utf8"));
    assert.equal(config.workers_dev, false);
    assert.equal(config.preview_urls, false);
    assert.deepEqual(config.routes, []);
    assert.deepEqual(config.r2_buckets, [
      { binding: "MEDIA_BUCKET", bucket_name: "zoen-private-blobs" },
    ]);
  } finally {
    await mf.dispose();
  }
});

test("same ciphertext racing across two Directory instances creates once and preserves the original bytes", async () => {
  const mf = runtime();
  try {
    await seed(mf, "race-left");
    await seed(mf, "race-right");
    const results = await Promise.all([
      request(mf, first, "race-left"),
      request(mf, first, "race-right"),
    ]);
    assert.deepEqual(results.map((result) => result.status).sort(), [200, 201]);
    const read = await request(mf, { op: "get", sha: first.sha });
    assert.equal(read.body_base64, first.body.base64);
    assert.equal(digest(Buffer.from(read.body_base64, "base64")), first.sha);
  } finally {
    await mf.dispose();
  }
});

test("a corrupted duplicate is still refused and spends quota without overwriting its valid object", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    await request(mf);
    const before = await control(mf, "inspect-quota");
    const bad = {
      ...first,
      body: { base64: Buffer.from("changed ciphertext").toString("base64") },
    };
    const result = await request(mf, bad);
    assert.equal(result.status, 400);
    assert.equal(Buffer.from(result.body_base64, "base64").toString(), "hash mismatch");
    assert.ok(BigInt((await control(mf, "inspect-quota"))[0].tat_ns) > BigInt(before[0].tat_ns));
    assert.equal((await request(mf, { op: "get", sha: first.sha })).body_base64, first.body.base64);
  } finally {
    await mf.dispose();
  }
});

test("R2 failures never acknowledge an upload, and a late download-body failure becomes503 before headers", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    await control(mf, "configure", { failPut: true });
    assert.equal((await request(mf)).status, 503);
    assert.deepEqual(await control(mf, "objects"), []);
    await control(mf, "configure", { failPut: false });
    assert.equal((await request(mf)).status, 201);
    for (const failure of ["failGet", "readError"]) {
      await control(mf, "configure", { [failure]: true });
      const result = await request(mf, { op: "get", sha: first.sha });
      assert.equal(result.status, 503);
      assert.equal(Buffer.from(result.body_base64, "base64").toString(), "storage unavailable");
      assert.equal(result.headers.get("Cache-Control"), null);
      await control(mf, "configure", { [failure]: false });
      assert.equal(
        (await request(mf, { op: "get", sha: first.sha })).body_base64,
        first.body.base64,
      );
    }
  } finally {
    await mf.dispose();
  }
});

assert.equal(native.storage_failures.length, 3);
for (const failure of native.storage_failures) {
  test(`actual native ${failure.fault} storage failure has identical plaintext503/CORS and leaves ciphertext unchanged`, async () => {
    const mf = runtime();
    try {
      await seed(mf);
      assert.equal((await request(mf)).status, 201);
      await control(mf, "configure", {
        now_ms: failure.now_ms,
        [failure.fault === "put" ? "failPut" : failure.fault === "get" ? "failGet" : "readError"]:
          true,
      });
      sameResponse(await request(mf, failure), failure.expected, failure.fault);
      const objects = (await control(mf, "objects")).map(
        ({ metadata: _metadata, ...object }) => object,
      );
      assert.deepEqual(objects, failure.objects_after);
      assert.deepEqual(failure.objects_before, failure.objects_after);
    } finally {
      await mf.dispose();
    }
  });
}

test("an admitted R2 upload may complete after production Unlink, while every fresh or duplicate upload then403s", async () => {
  const mf = runtime();
  let pending;
  try {
    await seed(mf);
    await control(mf, "configure", { hold: true });
    pending = request(mf);
    await until(async () => (await control(mf, "configure")).held === 1, "Actual R2 put entered");
    await control(mf, "unlink", { now_ms: first.now_ms });
    await control(mf, "configure", { release: true });
    assert.equal((await pending).status, 201);
    const retry = await request(mf);
    assert.equal(retry.status, 403);
    assert.equal(Buffer.from(retry.body_base64, "base64").toString(), "unlinked device");
    assert.equal((await request(mf, { op: "get", sha: first.sha })).body_base64, first.body.base64);
  } finally {
    await control(mf, "configure", { release: true });
    await pending?.catch(() => {});
    await mf.dispose();
  }
});

test("one isolate body budget holds other cells unread and bounds the metadata-only FIFO", async () => {
  const mf = runtime();
  const pending = [];
  try {
    await seed(mf, "held");
    await seed(mf, "waiting");
    await control(mf, "configure", { hold: true }, "held");
    pending.push(request(mf, first, "held"));
    await until(
      async () => (await control(mf, "configure", {}, "held")).held === 1,
      "Held upload acquired the shared lease",
    );
    for (let index = 0; index < 8; index++) pending.push(request(mf, first, "waiting"));
    await new Promise((resolve) => setTimeout(resolve, 100));
    const state = await control(mf, "configure", {}, "waiting");
    assert.equal(state.reads, 0, "Queued bodies are not read even by another Directory instance");
    assert.equal(state.entered, 0);
    const rejected = await request(mf, first, "waiting");
    assert.equal(rejected.status, 503, "Ninth waiting request is refused before reading its body");
    assert.equal((await control(mf, "configure", {}, "waiting")).reads, 0);
    await control(mf, "configure", { release: true }, "held");
    const results = await Promise.all(pending);
    assert.equal(results.filter((result) => result.status === 201).length, 1);
    assert.ok(results.every((result) => result.status === 201 || result.status === 200));
  } finally {
    await control(mf, "configure", { release: true }, "held");
    await Promise.allSettled(pending);
    await mf.dispose();
  }
});

test("cancelling queued requests removes their waiting slots without releasing another request's lease", async () => {
  const mf = runtime();
  let pending;
  let next;
  try {
    await seed(mf);
    await control(mf, "configure", { hold: true });
    pending = request(mf);
    await until(async () => (await control(mf, "configure")).held === 1, "R2 upload held");
    const before = await control(mf, "inspect-quota");
    for (let index = 0; index < 10; index++) {
      const cancelled = await mf.dispatchFetch("https://private.fixture/cancel-queued", {
        method: "POST",
        body: JSON.stringify({
          sha: first.sha,
          headers: first.headers,
          body_base64: first.body.base64,
        }),
      });
      assert.equal(cancelled.status, 503);
      await cancelled.arrayBuffer();
    }
    assert.deepEqual(await control(mf, "inspect-quota"), before);
    assert.equal((await control(mf, "configure")).entered, 1);
    next = request(mf);
    await control(mf, "configure", { release: true });
    assert.equal((await pending).status, 201);
    assert.equal((await next).status, 200);
  } finally {
    await control(mf, "configure", { release: true });
    await Promise.allSettled([pending, next].filter(Boolean));
    await mf.dispose();
  }
});

test("an abandoned downstream GET loses its retained body lease at the explicit private idle deadline", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    assert.equal((await request(mf)).status, 201);
    assert.equal((await control(mf, "abandoned-get", { sha: first.sha })).status, 200);
    const started = Date.now();
    await new Promise((resolve) => setTimeout(resolve, 50));
    assert.equal(
      (await request(mf)).status,
      200,
      "The abandoned response cannot retain the isolate slot forever",
    );
    assert.ok(Date.now() - started >= 9000, "Actual production idle deadline was exercised");
  } finally {
    await mf.dispose();
  }
});

test("quota survives reconstruction and restart, clamps a backward wall clock and preserves exact shared refusal bytes", async () => {
  const persistence = await mkdtemp(join(tmpdir(), "zoen-media-local-"));
  let mf = runtime(persistence);
  try {
    await seed(mf);
    const refusal = native.rate_vectors.find((vector) => vector.expected.accepted === false);
    assert.ok(refusal);
    const now = Number(first.now_ms);
    const nowNs = BigInt(first.now_ms) * 1_000_000n;
    const translatedTat = nowNs + BigInt(refusal.input.tat_ns) - BigInt(refusal.input.now_ns);
    await control(mf, "quota", {
      device: native.device,
      tat_ns: String(translatedTat),
      last_ns: String(nowNs),
      expires_ms: now + 120_000,
    });
    await control(mf, "configure", { now_ms: first.now_ms });
    // The vector is anchored to this real signed request's millisecond. A
    // reconstructed host must use persisted nanoseconds, not refill on wake.
    const retry = { ...first.headers };
    const reconstructed = await mf.dispatchFetch("https://private.fixture/reconstruct", {
      method: "POST",
      body: JSON.stringify({
        now_ms: first.now_ms,
        sha: first.sha,
        headers: retry,
        body_base64: first.body.base64,
      }),
    });
    assert.equal(reconstructed.status, 429);
    assert.equal(reconstructed.headers.get("Retry-After"), refusal.expected.retry_after);
    assert.equal(await reconstructed.text(), refusal.expected.body);
    const before = await control(mf, "inspect-quota");
    await mf.dispose();
    mf = runtime(persistence);
    await control(mf, "configure", { now_ms: String(Number(first.now_ms) - 1) });
    assert.equal((await request(mf)).status, 429);
    assert.deepEqual(
      await control(mf, "inspect-quota"),
      before,
      "Refusal cannot mutate rate state",
    );
    assert.deepEqual(await control(mf, "objects"), []);
  } finally {
    await mf.dispose();
    await rm(persistence, { recursive: true, force: true });
  }
});
