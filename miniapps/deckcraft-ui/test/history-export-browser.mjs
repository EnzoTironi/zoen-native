import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "../../../apps/zoen/node_modules/esbuild/lib/main.js";
import {
  findBrowserExecutable,
  ownedBrowserResult,
} from "../../excalidraw-ui/test/browser-driver.mjs";
import { auditAssets } from "../audit-assets.mjs";
const [dist, output, runtimePath] = process.argv.slice(2);
if (!dist || !output || !runtimePath)
  throw Error(
    "Usage: history-export-browser.mjs EXISTING_PINNED_DIST OUTPUT_DIRECTORY PINNED_RUNTIME_JSON",
  );
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
await mkdir(output, { recursive: true });
const pinned = JSON.parse(await readFile(runtimePath, "utf8"));
const upstream = JSON.parse(
  await readFile(join(root, "miniapps/deckcraft-ui/upstream.json"), "utf8"),
);
assert.equal(pinned.revision, upstream.revision);
const assets = await auditAssets(resolve(dist), undefined, pinned.revision);
for (const kind of ["module", "wasm"]) {
  assert.equal(assets[kind].sha256, pinned[kind].sha256, `Exact current pinned ${kind}`);
  assert.equal(assets[kind].bytes, pinned[kind].bytes);
}
const browser = await findBrowserExecutable();
if (!browser) throw Error("Actual owned Chrome required");
const original = await readFile(
  join(root, "crates/client-core/src/app_binary_documents/fixtures/original.deckcraft"),
);
const changed = await readFile(
  join(root, "crates/client-core/src/app_binary_documents/fixtures/changed.deckcraft"),
);
const bundle = await build({
  absWorkingDir: root,
  entryPoints: ["miniapps/deckcraft-ui/test/history-export.fixture.ts"],
  bundle: true,
  nodePaths: [join(root, "apps/zoen/node_modules")],
  preserveSymlinks: true,
  platform: "browser",
  format: "esm",
  write: false,
  target: ["safari17"],
  define: { "process.env.NODE_ENV": '"production"' },
});
const files = new Map([
  ["/probe.js", { bytes: bundle.outputFiles[0].contents, mime: "text/javascript" }],
  ...(await Promise.all(
    ["module", "wasm"].map(async (kind) => [
      `/${assets[kind].file}`,
      {
        bytes: await readFile(join(dist, assets[kind].file)),
        mime: kind === "wasm" ? "application/wasm" : "text/javascript",
      },
    ]),
  )),
]);
const requests = [];
const server = createServer((request, response) => {
  requests.push({ method: request.method, path: request.url });
  const file = files.get(request.url);
  if (request.method !== "GET" || !file) {
    response.writeHead(404);
    response.end();
    return;
  }
  response.writeHead(200, {
    "Content-Type": file.mime,
    "Content-Length": file.bytes.length,
    "Cache-Control": "no-store",
  });
  response.end(file.bytes);
});
const temp = await mkdtemp(join(tmpdir(), "zoen-history-export-"));
let receipt;
try {
  await new Promise((done, fail) => {
    server.once("error", fail);
    server.listen(0, "127.0.0.1", done);
  });
  const origin = `http://localhost:${server.address().port}`;
  const html = `<!doctype html><meta charset="utf-8"><canvas id="deckcraft_canvas" style="width:430px;height:932px"></canvas><div id="deckcraft_loading"></div><script type="module">
    let reported=false;function report(value){if(reported)return;reported=true;const out=document.createElement('pre');out.id='result';out.textContent=JSON.stringify(value);document.body.appendChild(out);}
    window.addEventListener('error',()=>report({trap:true}));window.addEventListener('unhandledrejection',event=>report({fatal:String(event.reason)}));
    try { const runtime=await import('/${assets.module.file}');await runtime.default({module_or_path:await(await fetch('/${assets.wasm.file}')).arrayBuffer()});runtime.zoenHostStart();
      const {runHistoryExport}=await import('/probe.js');
      report({result:await runHistoryExport(runtime,new Uint8Array(${JSON.stringify([...original])}),new Uint8Array(${JSON.stringify([...changed])}))});
    }catch(error){report({fatal:String(error)});}
    </script>`;
  files.set("/history?webgl", { bytes: Buffer.from(html), mime: "text/html" });
  const page = join(temp, "start.html");
  await writeFile(
    page,
    `<!doctype html><script>location.replace(${JSON.stringify(origin + "/history?webgl")})</script>`,
  );
  const quote = (value) => `'${value.replaceAll("'", "'\\''")}'`;
  const executable = join(temp, "owned-chrome");
  await writeFile(
    executable,
    `#!/bin/sh\nexec ${quote(browser)} --enable-unsafe-swiftshader --use-angle=swiftshader "$@"\n`,
    { mode: 0o700 },
  );
  const observed = await ownedBrowserResult(executable, temp, page);
  const actual = JSON.parse(observed.text);
  assert.ok(actual.result, JSON.stringify(actual));
  assert.equal(actual.result.exports.length, 2);
  for (const file of actual.result.exports) {
    const bytes = Buffer.from(file.data, "base64");
    assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
    await writeFile(join(output, file.filename), bytes);
    delete file.data;
    file.bytes = bytes.length;
  }
  await writeFile(join(output, "browser.png"), Buffer.from(observed.png, "base64"));
  assert.equal(observed.lifecycle.closeObserved, true);
  assert.equal(observed.lifecycle.stopped, true);
  receipt = {
    status: "PASS",
    scope:
      "Actual pinned browser JS/WASM + official SDK + native session/document/export adapters with synthetic authorized Core read port. Not Rust Core, WKWebView or iPhone QA.",
    browserVersion: execFileSync(browser, ["--version"], { encoding: "utf8" }).trim(),
    assets,
    probeSha256: createHash("sha256").update(bundle.outputFiles[0].contents).digest("hex"),
    ...actual,
    requests,
    lifecycle: observed.lifecycle,
    console: observed.errors,
  };
} catch (error) {
  receipt = {
    status: "FAIL",
    message: String(error),
    browser: error.ownedBrowser,
    requests,
    assets,
  };
  throw error;
} finally {
  server.closeAllConnections();
  if (server.listening) await new Promise((done) => server.close(done));
  await rm(temp, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  if (receipt)
    await writeFile(join(output, "result.json"), JSON.stringify(receipt, null, 2) + "\n");
}
