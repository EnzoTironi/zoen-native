import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { test } from "node:test";
import { auditAssets } from "../audit-assets.mjs";

const revision = "a".repeat(40);
const exports =
  "export default async()=>{};export function zoenHostStart(){};export function zoenHostRequest(){};export function zoenHostOnChange(){};export function zoenHostRetire(){}";
const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const moduleName = "deckcraft-web-abcd.js";
const wasmName = "deckcraft-web-abcd_bg.wasm";
async function fixture(fn) {
  const path = await mkdtemp(resolve(tmpdir(), "zoen-deckcraft-assets-"));
  try {
    await writeFile(resolve(path, moduleName), exports);
    await writeFile(resolve(path, wasmName), wasm);
    await fn(path);
  } finally {
    await rm(path, { recursive: true });
  }
}
test("release audit derives exact bytes and hashes from a real JS/WASM pair without executing it", async () => {
  await fixture(async (path) => {
    const manifest = await auditAssets(path, "https://code.example.test/deckcraft/", revision);
    assert.equal(manifest.module.bytes, Buffer.byteLength(exports));
    assert.equal(manifest.module.sha256, digest(exports));
    assert.equal(manifest.wasm.sha256, digest(wasm));
    assert.equal(manifest.wasm.url, `https://code.example.test/deckcraft/${revision}/${wasmName}`);
    assert.equal(manifest.abi, "zoen-host-v1");
    const unpublished = await auditAssets(path, undefined, revision);
    assert.equal(unpublished.module.file, moduleName);
    assert.equal("url" in unpublished.module, false);
    assert.equal("url" in unpublished.wasm, false);
  });
});
test("release audit rejects standalone ABI, unchecked imports and invalid binaries", async () => {
  await fixture(async (path) => {
    for (const source of [
      exports.replace("zoenHostStart", "standaloneStart"),
      `import 'https://unchecked.example.test/module.js';${exports}`,
    ]) {
      await writeFile(resolve(path, moduleName), source);
      await assert.rejects(auditAssets(path, "https://code.example.test/deckcraft/", revision));
    }
    await writeFile(resolve(path, moduleName), exports);
    await writeFile(resolve(path, wasmName), Buffer.from("bad wasm"));
    await assert.rejects(
      auditAssets(path, "https://code.example.test/deckcraft/", revision),
      /WASM/,
    );
  });
});
test("pinned source patches and font licenses retain the exact recorded identities", async () => {
  const root = new URL("../", import.meta.url);
  const upstream = JSON.parse(await readFile(new URL("upstream.json", root), "utf8"));
  for (const patch of upstream.patches)
    assert.equal(digest(await readFile(new URL(patch.path, root))), patch.sha256);
  const fonts = JSON.parse(await readFile(new URL("font-input/upstream.json", root), "utf8"));
  for (const license of fonts.licenseFiles)
    assert.equal(
      digest(await readFile(new URL(`licenses/craft-fonts/${license.path}`, root))),
      license.sha256,
    );
});
