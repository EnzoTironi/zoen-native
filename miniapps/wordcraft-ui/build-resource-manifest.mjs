// Bind the configured resource to actual sealed local assets, never to Rust PINS.
import { createHash } from "node:crypto";
import { readFile, writeFile, lstat, realpath } from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseRuntimePackage } from "../../apps/zoen/src/miniapps/runtime-package.ts";
const root = dirname(fileURLToPath(import.meta.url));
const [runtimePath, assetsPath, outputPath] = process.argv.slice(2);
if (!runtimePath || !assetsPath || !outputPath)
  throw Error("Usage: node build-resource-manifest.mjs VERIFIED_RUNTIME_JSON SEALED_ASSET_DIRECTORY OUTPUT_JSON");
const runtimePackage = parseRuntimePackage(JSON.parse(await readFile(runtimePath, "utf8")));
const revision = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
if (runtimePackage.engine !== "wordcraft" || runtimePackage.revision !== revision)
  throw Error("The configured Word engine revision changed");
const directory = await realpath(assetsPath);
for (const asset of [runtimePackage.module, runtimePackage.wasm]) {
  const path = resolve(directory, basename(new URL(asset.url).pathname));
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size !== asset.bytes)
    throw Error("Runtime asset type or byte length mismatch");
  const bytes = await readFile(path);
  if (bytes.length !== asset.bytes || createHash("sha256").update(bytes).digest("hex") !== asset.sha256)
    throw Error("Runtime asset digest mismatch");
}
const html = await readFile(resolve(root, "../../crates/miniapps/resources/generated/wordcraft.html"));
const manifest = {
  id: "wordcraft", name: "WordCraft", resourceUri: "ui://zoen/wordcraft/index.html",
  mimeType: "text/html;profile=mcp-app", capabilities: [], allowedDomains: [],
  linkDomains: [], networkPurpose: "", permissions: {},
  csp: { connectDomains: [], resourceDomains: [], frameDomains: [], baseUriDomains: [] },
  sha256: createHash("sha256").update(html).digest("hex"), bytes: html.length, runtimePackage,
};
await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
