import { build } from "../../apps/zoen/node_modules/esbuild/lib/main.js";
import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseRuntimePackage } from "../../apps/zoen/src/miniapps/runtime-package.ts";

/** Audit actual outputs; a compiled module may never load unchecked JS dependencies. */
export async function auditAssets(directory, baseUrl, revision) {
  const files = await readdir(directory);
  const modules = files.filter((file) => /^deckcraft-web-[a-f0-9]+\.js$/.test(file));
  const binaries = files.filter((file) => /^deckcraft-web-[a-f0-9]+_bg\.wasm$/.test(file));
  if (modules.length !== 1 || binaries.length !== 1)
    throw Error("Expected one managed JS/WASM pair");
  const module = await readFile(resolve(directory, modules[0]));
  const wasm = await readFile(resolve(directory, binaries[0]));
  const moduleText = new TextDecoder("utf-8", { fatal: true }).decode(module);
  if (
    !/^[a-f0-9]{40}$/.test(revision) ||
    module.length < 1 ||
    module.length > 512 * 1024 ||
    wasm.length > 48 * 1024 * 1024
  )
    throw Error("Runtime package exceeds the pinned code budgets");
  if (!wasm.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 1, 0, 0, 0])))
    throw Error("Invalid WASM magic/version");
  const checked = await build({
    stdin: { contents: moduleText, sourcefile: modules[0], loader: "js" },
    bundle: true,
    write: false,
    metafile: true,
    platform: "browser",
    format: "esm",
    logLevel: "silent",
    plugins: [
      {
        name: "deny-imports",
        setup(api) {
          api.onResolve({ filter: /.*/ }, (args) => {
            throw Error(`Unchecked runtime import: ${args.path}`);
          });
        },
      },
    ],
  });
  if (
    checked.warnings.length ||
    Object.values(checked.metafile.outputs).some((item) => item.imports.length)
  )
    throw Error("Runtime glue must be self-contained with no unresolved dynamic imports");
  const exports = Object.values(checked.metafile.outputs).flatMap((item) => item.exports);
  for (const name of [
    "default",
    "zoenHostStart",
    "zoenHostRequest",
    "zoenHostOnChange",
    "zoenHostRetire",
  ])
    if (!exports.includes(name)) throw Error(`Missing managed ABI export: ${name}`);
  const asset = (name, bytes, mime) => ({
    ...(baseUrl ? { url: `${baseUrl}${revision}/${name}` } : { file: name }),
    bytes: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    mime,
  });
  const runtime = {
    schema: 1,
    engine: "deckcraft",
    revision,
    abi: "zoen-host-v1",
    module: asset(modules[0], module, "text/javascript"),
    wasm: asset(binaries[0], wasm, "application/wasm"),
  };
  return baseUrl ? parseRuntimePackage(runtime) : runtime;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const [directory, baseUrl, revision, destination] = process.argv.slice(2);
  if (!directory || !baseUrl || !revision || !destination)
    throw Error("Usage: node audit-assets.mjs DIST EXPLICIT_BASE_URL REVISION OUTPUT_JSON");
  const runtime = await auditAssets(directory, baseUrl === "-" ? undefined : baseUrl, revision);
  await writeFile(destination, `${JSON.stringify(runtime, null, 2)}\n`);
}
