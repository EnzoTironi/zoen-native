import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const root = fileURLToPath(new URL("../", import.meta.url));
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_TRANSFER_FIXTURE
      ? resolve(process.env.ZOEN_TRANSFER_FIXTURE)
      : join(root, "generated/transfer-fixture.json"),
    "utf8",
  ),
);
assert.equal(native.schema, 1);
assert.equal(native.scope, "native-transfer-http");
assert.ok(native.steps.length >= 30);
const first = native.steps.find((step) => step.label === "transfer-first");
const decode = (body) =>
  body?.repeat_byte === undefined
    ? Buffer.from(body?.base64 ?? "", "base64")
    : Buffer.alloc(Number(body.length), body.repeat_byte);
const hash = (body) => createHash("sha256").update(body).digest("hex");
function runtime() {
  return new Miniflare(
    convertV4MiniflareOptions({
      modulesRoot: root,
      modules: [
        "tests/transfer-fixture.mjs",
        "tests/media-fixture.mjs",
        "tests/directory-fixture.mjs",
        "src/directory-index.mjs",
        "src/relay-entrypoint.mjs", "src/relay-route.mjs", "src/directory-state.mjs", "src/relay-composition.mjs",
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
      bindings: { RELAY_NAME: native.relay, DIRECTORY_CELL: "private-transfer-fixture" },
      durableObjects: { DIRECTORY: { className: "DirectoryState", useSQLite: true } },
      r2Buckets: { MEDIA_BUCKET: "local-private-transfer" },
    }),
  );
}
async function control(mf, path, input) {
  const response = await mf.dispatchFetch(`https://private.fixture/${path}`, {
    method: "POST",
    body: JSON.stringify(input ?? {}),
  });
  assert.equal(response.status, 200, await response.clone().text());
  return response.json();
}
async function seed(mf) {
  return control(mf, "seed", { ...native, now_ms: first.now_ms });
}
async function request(mf, step = first, prefix = "") {
  return mf.dispatchFetch(`https://private.fixture${prefix}${step.path}`, {
    method: step.method,
    headers: step.headers,
    body: ["GET", "HEAD"].includes(step.method) ? undefined : decode(step.body),
  });
}
async function inventory(mf) {
  const bucket = await mf.getR2Bucket("MEDIA_BUCKET");
  const listed = await bucket.list();
  assert.equal(listed.truncated, false);
  const objects = [];
  for (const entry of listed.objects) {
    const object = await bucket.get(entry.key);
    assert.deepEqual(object.customMetadata, {});
    const body = Buffer.from(await object.arrayBuffer());
    objects.push({ key: entry.key, length: String(body.length), sha256: hash(body) });
  }
  return objects.sort((a, b) => a.key.localeCompare(b.key));
}

test("every native HTTP history-transfer response and stored byte matches private workerd/R2", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    for (const step of native.steps) {
      await control(mf, "configure", { now_ms: step.now_ms });
      if (step.op === "revoke") await control(mf, "unlink", step);
      else {
        const response = await request(mf, step);
        assert.equal(response.status, step.expected.status, step.label);
        assert.equal(
          Buffer.from(await response.arrayBuffer()).toString("base64"),
          step.expected.body_base64,
          step.label,
        );
        for (const [key, value] of Object.entries(step.expected.headers))
          assert.equal(response.headers.get(key), value, `${step.label}: ${key}`);
      }
      assert.deepEqual(await inventory(mf), step.objects_after, step.label);
    }
  } finally {
    await mf.dispose();
  }
});

test("fixed-cell transfer binding works while production HTTP stays private and ciphertext reads are never cached", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    assert.equal((await request(mf, first, "/transfer-facade")).status, 201);
    const response = await request(mf, { ...first, method: "GET" }, "/transfer-facade");
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("Cache-Control"), "no-store");
    assert.deepEqual(Buffer.from(await response.arrayBuffer()), decode(first.body));
    for (const path of [first.path, "/v1/sync", "/v1/auth/challenge"])
      assert.equal((await mf.dispatchFetch(`https://private.fixture/public${path}`)).status, 404);
    const head = await request(mf, { ...first, method: "HEAD" });
    assert.equal(head.status, 200);
    assert.equal((await head.arrayBuffer()).byteLength, 0);
    assert.equal(head.headers.get("Cache-Control"), "no-store");
  } finally {
    await mf.dispose();
  }
});

test("storage put/get/late-read failure never becomes successful incomplete history", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    await control(mf, "configure", { failPut: true });
    assert.equal((await request(mf)).status, 503);
    assert.deepEqual(await inventory(mf), []);
    await control(mf, "configure", { failPut: false });
    assert.equal((await request(mf)).status, 201);
    for (const key of ["failGet", "readError"]) {
      await control(mf, "configure", { [key]: true });
      const response = await request(mf, { ...first, method: "GET" });
      assert.equal(response.status, 503);
      assert.equal(await response.text(), "storage unavailable");
      assert.equal(response.headers.get("Cache-Control"), null);
      await control(mf, "configure", { [key]: false });
    }
  } finally {
    await mf.dispose();
  }
});

test("history and media share the device rate budget, including correctly signed duplicates", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    assert.equal((await request(mf)).status, 201);
    const before = (await control(mf, "inspect-quota"))[0];
    assert.equal((await request(mf)).status, 200);
    const after = (await control(mf, "inspect-quota"))[0];
    assert.ok(BigInt(after.tat_ns) > BigInt(before.tat_ns));
    const media = native.steps.find((step) => step.label === "media-shares-quota");
    assert.equal((await request(mf, media)).status, 201);
    const mediaCharge = (await control(mf, "inspect-quota"))[0];
    assert.ok(BigInt(mediaCharge.tat_ns) > BigInt(after.tat_ns));
    const now = BigInt(first.now_ms) * 1_000_000n;
    await control(mf, "quota", {
      device: native.device,
      tat_ns: String(now + 120_000_000_000n),
      last_ns: String(now),
      expires_ms: Number(first.now_ms) + 120000,
    });
    const denied = await request(mf);
    assert.equal(denied.status, 429);
    assert.ok(Number(denied.headers.get("Retry-After")) >= 1);
    assert.equal((await request(mf, media)).status, 429);
    assert.equal((await inventory(mf)).length, 2);
  } finally {
    await mf.dispose();
  }
});

test("exact-prefix deletion handles multiple R2 pages and retains unrelated encrypted objects", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    const bucket = await mf.getR2Bucket("MEDIA_BUCKET");
    const payload = decode(first.body);
    for (let n = 0; n < 1001; n++) await bucket.put(`transfers/${native.id}/${n}`, payload);
    await bucket.put(`transfers/${native.other_id}/0`, payload);
    await bucket.put(`blobs/ab/${"ab".repeat(32)}`, payload);
    const deletion = native.steps.find((step) => step.label === "delete");
    await control(mf, "configure", { now_ms: deletion.now_ms });
    assert.equal((await request(mf, deletion)).status, 204);
    assert.equal((await bucket.list({ prefix: `transfers/${native.id}/` })).objects.length, 0);
    assert.ok(await bucket.get(`transfers/${native.other_id}/0`));
    assert.ok(await bucket.get(`blobs/ab/${"ab".repeat(32)}`));
  } finally {
    await mf.dispose();
  }
});

test("failed and partially completed deletion stays retryable and never acknowledges retained chunks", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    const bucket = await mf.getR2Bucket("MEDIA_BUCKET");
    await bucket.put(`transfers/${native.id}/0`, decode(first.body));
    await bucket.put(`transfers/${native.id}/1`, decode(first.body));
    await bucket.put(`transfers/${native.other_id}/0`, decode(first.body));
    const deletion = native.steps.find((step) => step.label === "delete");
    await control(mf, "configure", { now_ms: deletion.now_ms, failList: true });
    assert.equal((await request(mf, deletion)).status, 503);
    assert.equal((await bucket.list({ prefix: `transfers/${native.id}/` })).objects.length, 2);
    await control(mf, "configure", { failList: false, failDelete: true });
    assert.equal((await request(mf, deletion)).status, 503);
    assert.equal((await bucket.list({ prefix: `transfers/${native.id}/` })).objects.length, 2);
    await control(mf, "configure", { failDelete: false, partialDelete: true });
    assert.equal((await request(mf, deletion)).status, 503);
    assert.equal((await bucket.list({ prefix: `transfers/${native.id}/` })).objects.length, 1);
    assert.equal((await request(mf, deletion)).status, 204);
    assert.equal((await bucket.list({ prefix: `transfers/${native.id}/` })).objects.length, 0);
    assert.ok(await bucket.get(`transfers/${native.other_id}/0`));
  } finally {
    await mf.dispose();
  }
});

test("a retained transfer upload also fences media body readers in the same isolate", async () => {
  const mf = runtime();
  try {
    await seed(mf);
    await control(mf, "configure", { hold: true });
    const transfer = request(mf);
    const until = async (check) => {
      const end = Date.now() + 5000;
      while (!(await check())) {
        assert.ok(Date.now() < end, "native storage boundary was not reached");
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    };
    await until(async () => (await control(mf, "configure")).entered === 1);
    const media = request(
      mf,
      native.steps.find((step) => step.label === "media-shares-quota"),
    );
    // The fixture counts real media-stream pulls; an unread queued request has0.
    await new Promise((resolve) => setTimeout(resolve, 30));
    const held = await control(mf, "configure");
    assert.equal(held.entered, 1);
    assert.equal(held.reads, 0);
    await control(mf, "configure", { release: true });
    assert.equal((await transfer).status, 201);
    assert.equal((await media).status, 201);
    assert.ok((await control(mf, "configure")).reads > 0);
  } finally {
    await mf.dispose();
  }
});
