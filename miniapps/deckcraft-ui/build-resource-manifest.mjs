import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseRuntimePackage } from "../../apps/zoen/src/miniapps/runtime-package.ts";
const root = dirname(fileURLToPath(import.meta.url));
const [runtimePath, outputPath] = process.argv.slice(2);
if (!runtimePath || !outputPath)
  throw Error("Usage: node build-resource-manifest.mjs VERIFIED_RUNTIME_JSON OUTPUT_JSON");
const runtimePackage = parseRuntimePackage(JSON.parse(await readFile(runtimePath, "utf8")));
const upstream = JSON.parse(await readFile(resolve(root, "upstream.json"), "utf8"));
if (runtimePackage.revision !== upstream.revision)
  throw Error("The configured engine revision changed");
const html = await readFile(resolve(root, "../../crates/miniapps/resources/deckcraft.html"));
const manifest = {
  id: "deckcraft",
  name: "DeckCraft",
  resourceUri: "ui://zoen/deckcraft/index.html",
  mimeType: "text/html;profile=mcp-app",
  capabilities: [],
  allowedDomains: [],
  linkDomains: [],
  networkPurpose: "",
  permissions: {},
  csp: { connectDomains: [], resourceDomains: [], frameDomains: [], baseUriDomains: [] },
  sha256: createHash("sha256").update(html).digest("hex"),
  bytes: html.length,
  runtimePackage,
};
await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
