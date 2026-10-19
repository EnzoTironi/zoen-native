import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import { bundledLibraryAssets } from "./library-assets.mjs";

const root = dirname(fileURLToPath(import.meta.url));
const output = join(root, "../../apps/zoen/src/miniapps/vendor");
const hash = (value) => createHash("sha256").update(value).digest("hex");
const upstream = JSON.parse(await readFile(join(root, "upstream.json"), "utf8"));
const assets = await bundledLibraryAssets(root);
const inputs = {};
for (const file of [
  ...Object.keys(upstream.sourceSha256),
  "upstream.json",
  "package.json",
  "package-lock.json",
  "build.mjs",
  "library-assets.mjs",
  "README.md",
  "NOTICE.md",
  "LICENSE",
  "src/canonical-app.tsx",
  "src/canonical-saves.ts",
  "src/canonical-transfer.ts",
  "licenses/excalidraw-v0.18.0-MIT.txt",
  "licenses/radix-primitives-MIT.txt",
  "licenses/fuzzy-MIT.txt",
  "patches/89-buffer-initial-events.diff",
  "patches/90-initial-host-context.diff",
].sort()) {
  const sha = hash(await readFile(join(root, file)));
  if (upstream.sourceSha256[file] && sha !== upstream.sourceSha256[file])
    throw new Error(`Unrecorded upstream source change: ${file}`);
  inputs[file] = sha;
}
const result = await build({
  absWorkingDir: root,
  entryPoints: ["src/mcp-entry.tsx"],
  outdir: "generated",
  bundle: true,
  metafile: true,
  write: false,
  format: "esm",
  platform: "browser",
  target: ["safari17"],
  jsx: "automatic",
  minify: true,
  legalComments: "inline",
  define: { "process.env.NODE_ENV": '"production"' },
  conditions: ["production"],
  plugins: [assets.plugin],
  loader: { ".woff2": "dataurl", ".woff": "dataurl", ".ttf": "dataurl" },
});
const packages = new Map();
for (const path of Object.keys(result.metafile.inputs)) {
  const match = path.match(/^node_modules\/((?:@[^/]+\/)?[^/]+)(?:\/|$)/);
  if (!match || packages.has(match[1])) continue;
  const base = join(root, "node_modules", match[1]);
  const pkg = JSON.parse(await readFile(join(base, "package.json"), "utf8"));
  let licenseText;
  for (const name of ["LICENSE", "LICENSE.md", "LICENSE.txt", "license", "license.md"]) {
    try {
      licenseText = await readFile(join(base, name), "utf8");
      break;
    } catch {}
  }
  if (!licenseText && pkg.name === "@excalidraw/excalidraw")
    licenseText = await readFile(join(root, "licenses/excalidraw-v0.18.0-MIT.txt"), "utf8");
  if (!licenseText && pkg.name.startsWith("@radix-ui/"))
    licenseText = await readFile(join(root, "licenses/radix-primitives-MIT.txt"), "utf8");
  if (!licenseText && pkg.name === "fuzzy") {
    licenseText = await readFile(join(root, "licenses/fuzzy-MIT.txt"), "utf8");
    pkg.license = "MIT (upstream README)";
  }
  if (!licenseText && pkg.name === "react-remove-scroll-bar" && pkg.license === "MIT")
    licenseText =
      "Published package metadata and README declare MIT; the published package contains no standalone license text. See NOTICE.md.\n";
  if (!licenseText) throw new Error(`Missing bundled dependency license: ${pkg.name}`);
  packages.set(match[1], {
    name: pkg.name,
    version: pkg.version,
    license: pkg.license,
    text: licenseText,
  });
}
const bundled = [...packages.values()].sort((a, b) => a.name.localeCompare(b.name));
const notices = bundled
  .map(
    (pkg) =>
      `===== ${pkg.name}@${pkg.version} (${pkg.license}) =====\n${pkg.text
        .replaceAll("\r\n", "\n")
        .replace(/[\t ]+$/gm, "")
        .trim()}\n`,
  )
  .join("\n");
const noticesPath = join(root, "THIRD-PARTY-LICENSES.txt");
if (process.argv.includes("--check")) {
  if ((await readFile(noticesPath, "utf8")) !== notices)
    throw new Error("Stale third-party notices");
} else await writeFile(noticesPath, notices);

const js = result.outputFiles.find((file) => file.path.endsWith(".js")).text;
const css = result.outputFiles.find((file) => file.path.endsWith(".css")).text;
const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="color-scheme" content="light dark"><title>Excalidraw</title><style>${css}</style></head><body><script type="module">${js}</script></body></html>`;
if (Buffer.byteLength(html) > 8 * 1024 * 1024)
  throw new Error("Excalidraw resource exceeds the existing 8 MiB verifier bound.");
for (const output of Object.values(result.metafile.outputs))
  if (
    output.imports.some(
      (item) => item.external && !(item.kind === "url-token" && item.path.startsWith("data:")),
    )
  )
    throw new Error("External runtime import in private editor.");
const manifest = {
  resourceUri: "ui://zoen/excalidraw.html",
  mimeType: "text/html;profile=mcp-app",
  presentation: "pinned-upstream-patched-build",
  sha256: hash(html),
  bytes: Buffer.byteLength(html),
  csp: {
    connectDomains: [],
    resourceDomains: [],
    frameDomains: [],
    baseUriDomains: [],
  },
  permissions: {},
  provenance: {
    ...upstream,
    sourceInputs: inputs,
    fontAssetSha256: assets.hashes,
    runtimePolicy: {
      selfContained: true,
      optionalCjkFont: "Native system-ui fallback; all Unicode text retained",
      providerAiAndMermaid: false,
    },
    bundledDependencies: bundled.map(({ name, version, license }) => ({ name, version, license })),
  },
};
const files = {
  "excalidraw-ui.ts": `// Generated by miniapps/excalidraw-ui/build.mjs. See the adjacent integrity manifest.\n// prettier-ignore\nexport const manifest = ${JSON.stringify(manifest)} as const;\n// prettier-ignore\nexport default ${JSON.stringify(html)};\n`,
  "excalidraw-ui.manifest.json": execFileSync(
    join(root, "../../apps/zoen/node_modules/.bin/oxfmt"),
    ["--stdin-filepath", join(output, "excalidraw-ui.manifest.json")],
    { input: JSON.stringify(manifest, null, 2), encoding: "utf8" },
  ),
};
for (const [name, content] of Object.entries(files)) {
  const path = join(output, name);
  if (process.argv.includes("--check")) {
    if ((await readFile(path, "utf8")) !== content)
      throw new Error(`Stale Excalidraw UI artifact: ${name}`);
  } else {
    await mkdir(output, { recursive: true });
    await writeFile(path, content);
  }
}
const rustOutput = join(root, "../../crates/miniapps/resources");
const rustManifest = {
  ...manifest,
  id: "excalidraw",
  name: "Excalidraw",
  capabilities: [],
  allowedDomains: [],
  linkDomains: [],
  networkPurpose: "",
};
for (const [name, content] of Object.entries({
  "excalidraw.html": html,
  "excalidraw.manifest.json": JSON.stringify(rustManifest, null, 2) + "\n",
})) {
  const path = join(rustOutput, name);
  if (process.argv.includes("--check")) {
    if ((await readFile(path, "utf8")) !== content) throw new Error(`Stale Rust resource: ${name}`);
  } else await writeFile(path, content);
}
console.log(
  JSON.stringify({
    resourceUri: manifest.resourceUri,
    sha256: manifest.sha256,
    bytes: manifest.bytes,
    check: process.argv.includes("--check"),
  }),
);
