#!/usr/bin/env node
// Inspect the actual Expo export; this never starts a browser or injects app state.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const hash = (bytes, algorithm = "sha256") => createHash(algorithm).update(bytes).digest("hex");
const integrity = (bytes) => `sha384-${createHash("sha384").update(bytes).digest("base64")}`;
async function files(directory) {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...(await files(path)));
    else if (entry.isFile()) result.push(path);
  }
  return result.sort();
}
export async function inspectExpoWebArtifact(directory = join(root, "apps/zoen/dist")) {
  const exported = resolve(directory);
  const worker = await readFile(join(exported, "worker.js"));
  const wasm = await readFile(join(exported, "core/zoen_web_core_bg.wasm"));
  const expected = JSON.parse(
    await readFile(join(root, "apps/zoen/src/core/generated/web-assets.json"), "utf8"),
  );
  const headers = await readFile(join(exported, "_headers"));
  assert.equal(
    headers.toString(),
    await readFile(join(root, "apps/zoen/public/_headers"), "utf8"),
    "export must retain exact Cloudflare policy",
  );
  assert.equal(
    await readFile(join(exported, "_redirects"), "utf8"),
    await readFile(join(root, "apps/zoen/public/_redirects"), "utf8"),
    "export must retain canonical OAuth alias redirects",
  );
  assert.equal(
    integrity(worker),
    expected.workerIntegrity,
    "exported worker differs from the application-pinned worker",
  );
  assert.ok(
    worker.includes(Buffer.from(integrity(wasm))),
    "worker must pin the exported WASM bytes",
  );
  const index = await readFile(join(exported, "index.html"), "utf8");
  assert.match(index, /<html[\s>]/i, "release app entry is missing");
  assert.match(
    index,
    /<script[\s>]/i,
    "release entry must reference the actual Expo client bundle",
  );
  const manifest = [];
  let bundlePinsWorker = false;
  for (const path of await files(exported)) {
    const bytes = await readFile(path);
    const name = relative(exported, path).split("\\").join("/");
    if (
      name !== "worker.js" &&
      name.endsWith(".js") &&
      bytes.includes(Buffer.from(expected.workerIntegrity))
    )
      bundlePinsWorker = true;
    manifest.push({ path: name, bytes: bytes.length, sha256: hash(bytes) });
  }
  assert.ok(
    bundlePinsWorker,
    "release application must use this worker integrity, not stale development assets",
  );
  assert.ok(
    manifest.some((file) => file.path.startsWith("_expo/") && file.path.endsWith(".js")),
    "Expo release chunks are missing",
  );
  assert.ok(
    !manifest.some((file) => file.path === "404.html"),
    "SPA export must not disable Pages fallback with a top-level 404.html",
  );
  return { workerIntegrity: expected.workerIntegrity, wasmBytes: wasm.length, manifest };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = await inspectExpoWebArtifact(process.argv[2]);
  console.log(JSON.stringify(result, null, 2));
}
