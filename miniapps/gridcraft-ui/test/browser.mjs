import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";
import { sandboxHtml } from "../../../apps/zoen/src/miniapps/sandbox.ts";
import { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";
import { childHtml } from "./browser-child.mjs";

const [dist, seedPath, outputPath, playwrightModule] = process.argv.slice(2);
if (![dist, seedPath, outputPath, playwrightModule].every((v) => v && isAbsolute(v)))
  throw Error("Usage: browser.mjs ABS_PINNED_DIST ABS_ACTUAL_ENGINE_SEED ABS_OUTPUT ABS_PLAYWRIGHT_MODULE");
const output = resolve(outputPath);
await mkdir(output, { recursive: true, mode: 0o700 });
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const moduleBytes = await readFile(resolve(dist, "gridcraft.js"));
const wasm = await readFile(resolve(dist, "gridcraft_bg.wasm"));
const seed = await readFile(seedPath);
assert.ok(moduleBytes.length > 0 && moduleBytes.length <= 512 * 1024);
assert.ok(wasm.length > 8 && wasm.length <= 48 * 1024 * 1024);
assert.deepEqual([...wasm.subarray(0, 8)], [0, 97, 115, 109, 1, 0, 0, 0]);
assert.ok(seed.length > 0 && seed.length < 512 * 1024);
const revision = "b8771d51fb4da60bd48c13cf313d6d6014dc5e66";
const asset = (bytes, ext, mime) => ({
  url: `https://code.example.test/gridcraft/${revision}/${sha(bytes)}.${ext}`,
  bytes: bytes.length,
  sha256: sha(bytes),
  mime,
});
const runtime = {
  schema: 1, engine: "gridcraft", revision, abi: "zoen-host-v1",
  module: asset(moduleBytes, "js", "text/javascript"),
  wasm: asset(wasm, "wasm", "application/wasm"),
};
function sheetCells(path) {
  const code = `import sys,zipfile,json,xml.etree.ElementTree as E
with zipfile.ZipFile(sys.argv[1]) as z:
 raw=z.read('xl/worksheets/sheet1.xml')
 if len(raw)>4*1024*1024: raise ValueError('fixture part limit')
 root=E.fromstring(raw); ns={'s':'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
 print(json.dumps({c.attrib['r']:{'value':c.findtext('s:v',default='',namespaces=ns),'formula':c.findtext('s:f',default='',namespaces=ns)}for c in root.findall('.//s:c',ns)}))`;
  return JSON.parse(execFileSync('python3', ['-c', code, path], {encoding:'utf8',timeout:10000}));
}
const child = sandboxHtml(childHtml(seed.toString("base64"), sha(seed)), [], runtime);
const requests = [], errors = [], consoleErrors = [], cleanupErrors = [];
const files = new Map([
  ["/module.js", [moduleBytes, "text/javascript"]],
  ["/engine.wasm", [wasm, "application/wasm"]],
  ["/favicon.ico", [Buffer.alloc(0), "image/x-icon"]],
]);
const html = `<!doctype html><style>html,body{margin:0}iframe{width:100vw;height:100vh;border:0}</style><iframe sandbox="allow-scripts"></iframe><script>
(async()=>{const frame=document.querySelector('iframe');
const [module,wasm]=await Promise.all([fetch('/module.js').then(r=>r.text()),fetch('/engine.wasm').then(r=>r.arrayBuffer())]);
let sent=false;window.addEventListener('message',e=>{if(!sent&&e.source===frame.contentWindow&&e.origin==='null'&&e.data?.zoenRuntime===1&&e.data.kind==='load'){sent=true;const bytes=new Uint8Array(wasm);frame.contentWindow.postMessage({zoenRuntime:1,kind:'ready',revision:${JSON.stringify(revision)},module,wasm:bytes},'*',[bytes.buffer]);}});
frame.srcdoc=${JSON.stringify(child).replaceAll("</script", "<\\/script")};})();</script>`;
files.set("/", [Buffer.from(html), "text/html"]);
const server = createServer((request, response) => {
  requests.push({ method: request.method, path: request.url });
  const value = files.get(request.url);
  if (request.method !== "GET" || !value) { response.writeHead(404); response.end(); return; }
  response.writeHead(200, { "Content-Type": value[1], "Content-Length": value[0].length, "Cache-Control": "no-store" });
  response.end(value[0]);
});
const { chromium } = await import(pathToFileURL(playwrightModule).href);
const executablePath = await findBrowserExecutable();
assert.ok(executablePath, "Actual owned Chrome required");
let browser, page, receipt, result = "FAIL";
try {
  await new Promise((done, fail) => { server.once("error", fail); server.listen(0, "127.0.0.1", done); });
  browser = await chromium.launch({ executablePath, headless: true, timeout: 30_000, args: ["--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--disable-background-networking", "--disable-component-update"] });
  page = await browser.newPage({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (e) => { if (e.type() === "error") consoleErrors.push(e.text()); });
  await page.goto(`http://127.0.0.1:${server.address().port}/`, { waitUntil: "load" });
  const frame = page.frames().find((f) => f !== page.mainFrame());
  assert.ok(frame);
  await frame.waitForFunction(() => window.probe?.ready || window.probe?.failed, undefined, { timeout: 18_000 });
  assert.equal(await frame.evaluate(() => window.probe.failed), undefined);
  const original = await frame.evaluate(() => window.probe.export());
  assert.equal(original.metadata.sha256, sha(seed));
  assert.equal(original.data, seed.toString("base64"), "Cold selected export is exact native bytes");
  await frame.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  await page.screenshot({ path: resolve(output, "opened.png") });
  // Real browser keyboard events into the actual upstream canvas/input handler.
  await frame.locator("#gridcraft_canvas").focus();
  await page.keyboard.type("7");
  await page.keyboard.press("Enter");
  await frame.waitForFunction(() => window.probe.changes.some((s) => s.dirty), undefined, { timeout: 5000 });
  const manual = await frame.evaluate(() => window.probe.snapshot("browser-manual"));
  assert.equal(manual.metadata.operation.kind, "humanCheckpoint");
  assert.equal(sha(Buffer.from(manual.data, "base64")), manual.metadata.sha256);
  assert.equal((await frame.evaluate(() => window.probe.status())).frozen, true);
  await assert.rejects(frame.evaluate(() => window.probe.wrongAccept()), /receipt_mismatch/);
  await page.keyboard.type("9"); await page.keyboard.press("Enter");
  await frame.evaluate(() => window.probe.freeze(true));
  const accepted = await frame.evaluate(() => window.probe.accept());
  assert.equal(accepted.frozen, true, "Durable ACK must not undo the close/flush pause");
  const manualExport = await frame.evaluate(() => window.probe.export());
  assert.equal(manualExport.data, manual.data);
  await writeFile(resolve(output, "manual.xlsx"), Buffer.from(manual.data, "base64"));
  const manualCells=sheetCells(resolve(output, "manual.xlsx"));
  assert.equal(Number(manualCells.A1?.value),7,"Real browser keyboard committed to A1");
  assert.ok(!manualCells.A2?.value,"Pending keyboard input did not write A2");
  await frame.evaluate(() => window.probe.freeze(false));
  const command = await frame.evaluate(() => window.probe.command("browser-command", "Sheet1", "A2", "=A1*2"));
  const repeated = await frame.evaluate(() => window.probe.command("browser-command", "Sheet1", "A2", "=A1*2"));
  assert.deepEqual(repeated, command, "Exact pending command retries never reexecute");
  const commandAccepted = await frame.evaluate(() => window.probe.accept());
  const exported = await frame.evaluate(() => window.probe.export());
  assert.equal(exported.data, command.data);
  await writeFile(resolve(output, "command.xlsx"), Buffer.from(exported.data, "base64"));
  const commandCells=sheetCells(resolve(output,"command.xlsx"));
  assert.equal(Number(commandCells.A1?.value),7);
  assert.equal(commandCells.A2?.formula,"A1*2");
  assert.equal(Number(commandCells.A2?.value),14,"Actual WASM calculation is exported");
  await frame.evaluate(({ data, head }) => window.probe.reopen(data, head), { data: exported.data, head: commandAccepted.head });
  assert.equal((await frame.evaluate(() => window.probe.export())).data, exported.data);
  for (const [width, height] of [[844, 390], [390, 460], [390, 844]]) {
    await page.setViewportSize({ width, height });
    assert.equal((await frame.evaluate(([w, h]) => window.probe.layout(w, h), [width, height])).layoutAccepted, true);
    assert.equal((await frame.evaluate(() => window.probe.export())).data, exported.data);
  }
  await assert.rejects(frame.evaluate(() => window.probe.command("external", "Sheet1", "A3", '=WEBSERVICE("https://example.invalid")')), /unavailable/);
  assert.equal((await frame.evaluate(() => window.probe.export())).data, exported.data);
  await page.screenshot({ path: resolve(output, "edited.png") });
  await frame.evaluate(() => window.probe.retire());
  await assert.rejects(frame.evaluate(() => window.probe.status()), /retired|unavailable/);
  await assert.rejects(frame.evaluate(() => window.__zoenRuntime.load().then((e) => e.zoenHostStart())), /retired/);
  assert.deepEqual(errors, []);
  assert.deepEqual(consoleErrors, []);
  assert.ok(!requests.some((r) => !["/", "/module.js", "/engine.wasm", "/favicon.ico"].includes(r.path)), "No document/formula/engine network traffic");
  receipt = { runtime, manualHash: manual.metadata.sha256, commandHash: exported.metadata.sha256, commandAccepted, manualCells, commandCells, requests, errors, consoleErrors, scope: "actual opaque-sandbox browser engine ABI/manual input/export; synthetic receipts, not Core or agent admission" };
  result = "PASS";
} catch (error) {
  receipt = { error: String(error), stack: error.stack, requests, errors, consoleErrors };
  if (page) await page.screenshot({ path: resolve(output, "failure.png") }).catch(() => {});
} finally {
  if (browser) await browser.close().catch((e) => cleanupErrors.push(String(e)));
  await new Promise((done) => server.close(done));
  await writeFile(resolve(output, "result.json"), JSON.stringify({ result, ...receipt, cleanupErrors }, null, 2) + "\n");
}
if (result !== "PASS" || cleanupErrors.length) throw Error("Actual Grid browser qualification failed; see result.json");
