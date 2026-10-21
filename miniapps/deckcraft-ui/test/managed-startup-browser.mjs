import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import {
  findBrowserExecutable,
  ownedBrowserResult,
} from "../../excalidraw-ui/test/browser-driver.mjs";
import { auditAssets } from "../audit-assets.mjs";
const [dist, receipt, seedPath] = process.argv.slice(2);
if (!dist || !receipt || !seedPath)
  throw Error(
    "Usage: node managed-startup-browser.mjs ACTUAL_MANAGED_DIST RECEIPT_JSON SYNTHETIC_EMPTY_SEED",
  );
const chrome = await findBrowserExecutable();
if (!chrome) throw Error("A fresh owned Chrome process is required.");
const browserVersion = execFileSync(chrome, ["--version"], { encoding: "utf8" }).trim();
const revision = JSON.parse(
  await readFile(new URL("../upstream.json", import.meta.url), "utf8"),
).revision;
const assets = await auditAssets(resolve(dist), undefined, revision);
const module = await readFile(join(dist, assets.module.file));
const wasm = await readFile(join(dist, assets.wasm.file));
const seed = await readFile(seedPath);
assert.ok(seed.length < 16384);
const seedSha = createHash("sha256").update(seed).digest("hex");
const requests = [];
const fixtures = new Map();
const only = new Map([
  [`/${assets.module.file}`, { bytes: module, mime: "text/javascript" }],
  [`/${assets.wasm.file}`, { bytes: wasm, mime: "application/wasm" }],
]);
const server = createServer((request, response) => {
  requests.push({ method: request.method, path: request.url });
  const asset = only.get(request.url) ?? fixtures.get(request.url);
  if (request.method !== "GET" || !asset) {
    response.writeHead(404);
    response.end();
    return;
  }
  response.writeHead(200, {
    "Content-Type": asset.mime,
    "Content-Length": asset.bytes.length,
    "Cache-Control": "no-store",
    "Access-Control-Allow-Origin": "*",
    "X-Content-Type-Options": "nosniff",
  });
  response.end(asset.bytes);
});
const folder = await mkdtemp(join(tmpdir(), "zoen-managed-startup-"));
const observations = [];
let failure;
try {
  await new Promise((done, fail) => {
    server.once("error", fail);
    server.listen(0, "127.0.0.1", done);
  });
  const port = server.address().port;
  const origin = `http://localhost:${port}`;
  // Owned wrapper only adds software WebGL for this GPU-independent startup oracle.
  // It execs the real Chrome, retaining the driver's exact process-group cleanup.
  const shellQuote = (value) => `'${value.replaceAll("'", "'\\''")}'`;
  const executable = join(folder, "owned-chrome");
  await writeFile(
    executable,
    `#!/bin/sh\nexec ${shellQuote(chrome)} --enable-unsafe-swiftshader --use-angle=swiftshader "$@"\n`,
    { mode: 0o700 },
  );
  for (const healthy of [false, true]) {
    const runFolder = join(folder, healthy ? "healthy" : "missing");
    await import("node:fs/promises").then((fs) => fs.mkdir(runFolder));
    const html = `<!doctype html><html><head><meta charset="utf-8"><title>Owned managed startup</title></head><body>${healthy ? '<canvas id="deckcraft_canvas" style="width:430px;height:932px"></canvas><div id="deckcraft_loading"></div>' : ""}<script type="module">
   let reported=false;function report(value){if(reported)return;reported=true;const out=document.createElement('pre');out.id='result';out.textContent=JSON.stringify(value);document.body.appendChild(out);}
   window.addEventListener('error',()=>report({trap:true}));
   window.addEventListener('unhandledrejection',()=>report({trap:true}));
   try{
    const runtime=await import(${JSON.stringify(`${origin}/${assets.module.file}`)});
    await runtime.default({module_or_path:await(await fetch(${JSON.stringify(`${origin}/${assets.wasm.file}`)})).arrayBuffer()});
    runtime.zoenHostStart();
    const layout=runtime.zoenHostRequest(JSON.stringify({op:'layout',input:{width:430,height:932,insets:{top:62,right:0,bottom:34,left:0},closeButton:{x:374,y:70,width:44,height:44}}}),new Uint8Array());
    const open=runtime.zoenHostRequest(JSON.stringify({op:'open',input:{scope:{artifactId:'synthetic-only',actorId:'synthetic-actor',actorKind:'human'},head:{version:'1',sha256:${JSON.stringify(seedSha)}}}}),new Uint8Array(${healthy ? JSON.stringify([...seed]) : "[]"}));
    const pending=await Promise.allSettled([layout,open]);
    let future;try{const status=await runtime.zoenHostRequest(JSON.stringify({op:'status'}),new Uint8Array());future={accepted:JSON.parse(status.metadata)};}catch(error){future={rejected:String(error)};}
    runtime.zoenHostRetire();
    report({pending:pending.map(result=>result.status==='rejected'?{status:result.status,reason:String(result.reason)}:{status:result.status,metadata:JSON.parse(result.value.metadata)}),future,canvasAbsent:!document.getElementById('deckcraft_canvas')});
   }catch(error){report({fatal:String(error)});}
   </script></body></html>`;
    const page = join(runFolder, "fixture.html");
    const fixturePath = `/probe-${healthy ? "healthy" : "missing"}?webgl`;
    fixtures.set(fixturePath, { bytes: Buffer.from(html), mime: "text/html" });
    await writeFile(
      page,
      `<!doctype html><script>location.replace(${JSON.stringify(origin + fixturePath)})</script>`,
    );
    const browser = await ownedBrowserResult(executable, runFolder, page);
    const actual = JSON.parse(browser.text);
    const pngPath = receipt.replace(/\.json$/, "") + `-${healthy ? "healthy" : "missing"}.png`;
    await writeFile(pngPath, Buffer.from(browser.png, "base64"));
    observations.push({
      pngPath,
      healthy,
      result: actual,
      console: browser.errors,
      lifecycle: browser.lifecycle,
      codeSha256: createHash("sha256").update(html).digest("hex"),
    });
    assert.equal(actual.canvasAbsent, !healthy);
    if (!healthy) {
      assert.deepEqual(
        actual.pending.map((result) => result.status),
        ["rejected", "rejected"],
      );
      for (const result of actual.pending)
        assert.equal(JSON.parse(result.reason).code, "canvas_missing");
      assert.equal(JSON.parse(actual.future.rejected).code, "canvas_missing");
    } else {
      assert.deepEqual(
        actual.pending.map((result) => result.status),
        ["fulfilled", "fulfilled"],
      );
      assert.equal(actual.pending[0].metadata.layoutAccepted, true);
      assert.equal(actual.pending[1].metadata.head.sha256, seedSha);
      assert.equal(actual.future.accepted.head.sha256, seedSha);
      assert.equal(actual.future.accepted.dirty, false);
    }
    assert.equal(browser.lifecycle.closeObserved, true);
    assert.equal(browser.lifecycle.stopped, true);
  }
  const codeRequests = requests.filter((request) => only.has(request.path));
  assert.equal(codeRequests.length, 4);
  assert.equal(new Set(codeRequests.map((request) => request.path)).size, 2);
  assert.equal(requests.filter((request) => fixtures.has(request.path)).length, 2);
  await writeFile(
    receipt,
    JSON.stringify(
      {
        status: "PASS",
        scope:
          "Actual built managed JS/WASM missing-canvas +healthy canonical synthetic seed controls; not WK/prod/browser acceptance",
        assets,
        seed: { bytes: seed.length, sha256: seedSha },
        browserVersion,
        nodeVersion: process.version,
        requests,
        observations,
      },
      null,
      2,
    ) + "\n",
  );
} catch (error) {
  failure = error;
  await writeFile(
    receipt,
    JSON.stringify(
      {
        status: "FAIL",
        browserVersion,
        nodeVersion: process.version,
        seed: { bytes: seed.length, sha256: seedSha },
        message: error.message,
        browser: error.ownedBrowser,
        requests,
        assets,
        observations,
      },
      null,
      2,
    ) + "\n",
  );
} finally {
  server.closeAllConnections();
  if (server.listening) await new Promise((done) => server.close(done));
  await rm(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
if (failure) throw failure;
