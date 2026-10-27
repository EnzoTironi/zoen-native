import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve, isAbsolute } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { sandboxHtml } from "../../../apps/zoen/src/miniapps/sandbox.ts";
import { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";
import { childHtml } from "./browser-command-child.mjs";

const [modulePath, wasmPath, seedPath, expectedPath, output, playwrightPath] = process.argv.slice(2);
if (![modulePath, wasmPath, seedPath, expectedPath, output, playwrightPath].every((p) => p && isAbsolute(p)))
  throw Error("Usage: browser-command.mjs ABS_MODULE ABS_WASM ABS_SEED ABS_NATIVE_ROTATED ABS_NEW_OUTPUT ABS_PLAYWRIGHT");
await mkdir(output, { mode: 0o700 });
const sha = (b) => createHash("sha256").update(b).digest("hex");
const module = await readFile(modulePath), wasm = await readFile(wasmPath);
const seed = await readFile(seedPath), expected = await readFile(expectedPath);
assert.ok(module.length <= 512 * 1024 && wasm.length <= 48 * 1024 * 1024);
const revision = "9b9643764f3fb77d9f0e48d7510983ae50621e11";
const asset = (b, suffix, mime) => ({ url: `https://qualification.invalid/photocraft/${revision}/${sha(b)}.${suffix}`, bytes: b.length, sha256: sha(b), mime });
const runtime = { schema: 1, engine: "photocraft", revision, abi: "zoen-host-v1", module: asset(module, "js", "text/javascript"), wasm: asset(wasm, "wasm", "application/wasm") };
function parentHtml(bytes, version) {
  const child = sandboxHtml(childHtml(bytes.toString("base64"), sha(bytes), version), [], runtime);
  return Buffer.from(`<!doctype html><style>html,body{margin:0}iframe{width:100vw;height:100vh;border:0}</style><iframe sandbox="allow-scripts"></iframe><script>
(async()=>{const frame=document.querySelector('iframe');
const [module,wasm]=await Promise.all([fetch('/module.js').then(r=>r.text()),fetch('/engine.wasm').then(r=>r.arrayBuffer())]);
let sent=false;window.addEventListener('message',e=>{if(!sent&&e.source===frame.contentWindow&&e.origin==='null'&&e.data?.zoenRuntime===1&&e.data.kind==='load'){sent=true;const bytes=new Uint8Array(wasm);frame.contentWindow.postMessage({zoenRuntime:1,kind:'ready',revision:${JSON.stringify(revision)},module,wasm:bytes},'*',[bytes.buffer]);}});
frame.srcdoc=${JSON.stringify(child).replaceAll("</script", "<\\/script")};})();</script>`);
}
const files = new Map([
  ["/module.js", [module, "text/javascript"]], ["/engine.wasm", [wasm, "application/wasm"]],
  ["/favicon.ico", [Buffer.alloc(0), "image/x-icon"]], ["/", [parentHtml(seed, "1"), "text/html"]],
]);
const errors = [], consoleErrors = [], cleanupErrors = [];
const server = createServer((req, res) => {
  const entry = files.get(req.url);
  if (req.method !== "GET" || !entry) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { "Content-Type": entry[1], "Content-Length": entry[0].length, "Cache-Control": "no-store" });
  res.end(entry[0]);
});
let browser, receipt = { status: "FAIL" };
try {
  await new Promise((done, fail) => { server.once("error", fail); server.listen(0, "127.0.0.1", done); });
  const { chromium, webkit, devices } = await import(pathToFileURL(playwrightPath).href);
  const isWebkit = process.env.ZOEN_PROBE_WEBKIT === "1";
  browser = await (isWebkit ? webkit : chromium).launch(isWebkit ? { headless: true, timeout: 30000 } : {
    executablePath: await findBrowserExecutable(), headless: true, timeout: 30000,
    args: ["--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--disable-background-networking", "--disable-component-update"],
  });
  const page = await browser.newPage({ ...(isWebkit ? devices["iPhone 13"] : {}), viewport: { width: 390, height: 844 }, deviceScaleFactor: 1, hasTouch: true });
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (e) => { if (e.type() === "error") consoleErrors.push(e.text()); });
  await page.goto(`http://127.0.0.1:${server.address().port}/`, { waitUntil: "load" });
  const ready = async () => {
    const frame = page.frames().find((f) => f.parentFrame()); assert.ok(frame);
    await frame.waitForFunction(() => window.probe?.ready || window.probe?.failed, undefined, { timeout: 25000 });
    assert.equal(await frame.evaluate(() => window.probe.failed), undefined);
    return frame;
  };
  let frame = await ready();
  const before = await frame.evaluate(() => window.probe.export("png"));
  await writeFile(resolve(output, "before.png"), Buffer.from(before.data, "base64"));
  const proposal = await frame.evaluate(() => window.probe.command("browser-photo-command", "rotate_clockwise"));
  assert.deepEqual(proposal.metadata.operation, { kind: "command", action: "rotate_clockwise" });
  assert.equal(proposal.metadata.sha256, sha(expected));
  assert.equal(proposal.data, expected.toString("base64"));
  assert.deepEqual(await frame.evaluate(() => window.probe.command("browser-photo-command", "rotate_clockwise")), proposal);
  await assert.rejects(frame.evaluate(() => window.probe.wrongAccept()));
  assert.equal((await frame.evaluate(() => window.probe.status())).head.version, "1");
  assert.equal((await frame.evaluate(() => window.probe.export())).data, seed.toString("base64"), "pending command cannot replace accepted export");
  const accepted = await frame.evaluate(() => window.probe.accept());
  assert.equal(accepted.head.version, "2");
  const checkpoint = await frame.evaluate(() => window.probe.snapshot("browser-photo-checkpoint"));
  assert.deepEqual(checkpoint.metadata.operation, { kind: "humanCheckpoint" });
  assert.equal(checkpoint.data, proposal.data);
  await frame.evaluate(() => window.probe.accept());
  await assert.rejects(frame.evaluate(() => window.probe.foreign()));
  await assert.rejects(frame.evaluate(() => window.probe.stale()));
  await assert.rejects(frame.evaluate(() => window.probe.command("unknown", "file.open")));
  const status = await frame.evaluate(() => window.probe.status());
  assert.equal(status.head.version, "3"); assert.equal(status.head.sha256, sha(expected));
  assert.equal(status.dirty, false); assert.equal(status.pendingMutationId, null);
  const png = await frame.evaluate(() => window.probe.export("png"));
  await writeFile(resolve(output, "rotated.png"), Buffer.from(png.data, "base64"));
  await writeFile(resolve(output, "rotated.pcraft"), expected);
  const pixels = JSON.parse(execFileSync(process.env.PHOTO_PYTHON ?? "python3", [fileURLToPath(new URL("./png-probe.py", import.meta.url)), "rotate-clockwise", resolve(output, "before.png"), resolve(output, "rotated.png")], { encoding: "utf8", timeout: 10000 }));
  await page.screenshot({ path: resolve(output, "command.png") });
  await frame.evaluate(() => window.probe.retire());
  await assert.rejects(frame.evaluate(() => window.probe.status()));
  files.set("/", [parentHtml(expected, "3"), "text/html"]);
  await page.reload({ waitUntil: "load" }); frame = await ready();
  const cold = await frame.evaluate(async () => ({ status: await window.probe.status(), counts: window.probe.counts, output: await window.probe.export() }));
  assert.equal(cold.status.head.version, "3"); assert.equal(cold.status.head.sha256, sha(expected));
  assert.equal(cold.output.data, expected.toString("base64"));
  assert.equal(cold.counts.command ?? 0, 0); assert.equal(cold.counts.snapshot ?? 0, 0); assert.equal(cold.counts.accept ?? 0, 0);
  await frame.evaluate(() => window.probe.retire());
  assert.deepEqual(errors, []); assert.deepEqual(consoleErrors, []);
  receipt = { status: "PASS", scope: "Actual pinned Photo browser renderer ABI with supplied synthetic scope/ACK; no Core persistence, certified agent, approval UI or device claim. Separate MCP bootstrap/manual Core-port journey remains independently tested.", browser: isWebkit ? "webkit" : "chromium", module: { bytes: module.length, sha256: sha(module) }, wasm: { bytes: wasm.length, sha256: sha(wasm) }, seedSha256: sha(seed), commandOperation: proposal.metadata.operation, accepted, checkpointOperation: checkpoint.metadata.operation, finalHead: status.head, independentPixels: pixels, coldReopen: { head: cold.status.head, mutationRequests: 0 }, errors, consoleErrors };
} catch (e) { receipt.failure = String(e?.stack ?? e); throw e; }
finally {
  try { await browser?.close(); } catch (e) { cleanupErrors.push(String(e)); }
  await new Promise((done) => server.close(done));
  receipt.cleanupErrors = cleanupErrors;
  await writeFile(resolve(output, "result.json"), `${JSON.stringify(receipt, null, 2)}\n`);
}
assert.deepEqual(cleanupErrors, []);
