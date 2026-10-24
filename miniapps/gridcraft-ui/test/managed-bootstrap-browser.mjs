import assert from "node:assert/strict";
import { exerciseWord, inspectWord, reopenWord } from "./word-bootstrap-assertions.mjs";
import { exerciseGrid, assertGridExport, reopenGrid } from "./grid-bootstrap-assertions.mjs";
import { installPressFocusRefusal, observePressFocusRefusal } from "./press-focus-policy.mjs";
import {
  installCanvasGestureFocusRefusal,
  assertCanvasGestureTextAdmission,
} from "./canvas-focus-admission.mjs";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "../../../apps/zoen/node_modules/esbuild/lib/main.js";
import { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";
const [editor, modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath, wordInspector] =
  process.argv.slice(2);
const refuseCanvasGesture = process.env.ZOEN_TEST_CANVAS_GESTURE_REFUSAL === "1";
const refusePressFocus = process.env.ZOEN_TEST_PRESS_FOCUS_REFUSAL === "1";
if ((refusePressFocus || refuseCanvasGesture) && editor !== "wordcraft")
  throw Error("The press-focus regression currently exercises the real Word editor");
if (
  !["deckcraft", "gridcraft", "wordcraft"].includes(editor) ||
  ![modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath].every(
    (p) => p && isAbsolute(p),
  )
)
  throw Error(
    "Usage: managed-bootstrap-browser.mjs EDITOR ABS_MODULE ABS_WASM ABS_HTML ABS_SEED ABS_OUTPUT ABS_PLAYWRIGHT [ABS_WORD_INSPECTOR]",
  );
if (editor === "wordcraft" && (!wordInspector || !isAbsolute(wordInspector)))
  throw Error("Word proof requires the genuine native Word inspector");
await mkdir(output, { recursive: true });
const sha = (b) => createHash("sha256").update(b).digest("hex");
const module = await readFile(modulePath),
  wasm = await readFile(wasmPath);
const html = await readFile(htmlPath, "utf8"),
  seed = await readFile(seedPath);
assert.ok(module.length <= 512 * 1024 && wasm.length <= 48 * 1024 * 1024);
const revision =
  editor === "deckcraft"
    ? "a31b990d32f2c8562f1588562f922960c3fde48b"
    : editor === "gridcraft"
      ? "b8771d51fb4da60bd48c13cf313d6d6014dc5e66"
      : "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
const asset = (b, suffix, mime) => ({
  url: `https://code.example.test/${editor}/${revision}/${sha(b)}.${suffix}`,
  bytes: b.length,
  sha256: sha(b),
  mime,
});
const config = {
  editor,
  html,
  seed: seed.toString("base64"),
  runtime: {
    schema: 1,
    engine: editor,
    revision,
    abi: "zoen-host-v1",
    module: asset(module, "js", "text/javascript"),
    wasm: asset(wasm, "wasm", "application/wasm"),
  },
};
const root = resolve(new URL("../../..", import.meta.url).pathname);
const bundled = await build({
  absWorkingDir: root,
  entryPoints: ["miniapps/gridcraft-ui/test/managed-bootstrap.fixture.ts"],
  bundle: true,
  write: false,
  format: "esm",
  platform: "browser",
  target: ["safari17"],
  preserveSymlinks: true,
  nodePaths: [resolve(root, "apps/zoen/node_modules")],
  define: { "process.env.NODE_ENV": '"production"', "process.env.EXPO_OS": '"web"' },
});
const files = new Map([
  ["/module.js", [module, "text/javascript"]],
  ["/engine.wasm", [wasm, "application/wasm"]],
  ["/probe.js", [bundled.outputFiles[0].contents, "text/javascript"]],
  ["/config.json", [Buffer.from(JSON.stringify(config)), "application/json"]],
  ["/favicon.ico", [Buffer.alloc(0), "image/x-icon"]],
  [
    "/",
    [
      Buffer.from(
        '<!doctype html><style>html,body{margin:0}iframe{width:100vw;height:100vh;border:0}</style><iframe sandbox="allow-scripts"></iframe><script type="module">import {startProbe} from "/probe.js";await startProbe(await(await fetch("/config.json")).json());</script>',
      ),
      "text/html",
    ],
  ],
]);
const requests = [],
  errors = [],
  consoleErrors = [],
  cleanupErrors = [];
const server = createServer((req, res) => {
  requests.push({ method: req.method, path: req.url });
  const f = files.get(req.url);
  if (req.method !== "GET" || !f) {
    res.writeHead(404);
    res.end();
    return;
  }
  res.writeHead(200, {
    "Content-Type": f[1],
    "Content-Length": f[0].length,
    "Cache-Control": "no-store",
  });
  res.end(f[0]);
});
let browser,
  receipt = { status: "FAIL" };
try {
  await new Promise((done, fail) => {
    server.once("error", fail);
    server.listen(0, "127.0.0.1", done);
  });
  const { chromium } = await import(pathToFileURL(playwrightPath).href);
  browser = await chromium.launch({
    executablePath: await findBrowserExecutable(),
    headless: true,
    timeout: 30000,
    args: [
      "--enable-unsafe-swiftshader",
      "--use-angle=swiftshader",
      "--disable-background-networking",
      "--disable-component-update",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 390, height: 844 },
    hasTouch: editor === "wordcraft",
  });
  if (refusePressFocus) await installPressFocusRefusal(page, "wordcraft_canvas");
  if (refuseCanvasGesture) await installCanvasGestureFocusRefusal(page, "wordcraft_canvas");
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (e) => {
    if (e.type() === "error") consoleErrors.push(e.text());
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/`, { waitUntil: "load" });
  await page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 25000 });
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  const formats =
    editor === "deckcraft" ? ["pptx", "pdf"] : editor === "gridcraft" ? ["xlsx"] : ["docx"];
  for (const format of formats) await page.evaluate((f) => window.probe.share(f), format);
  const word = editor === "wordcraft" ? await exerciseWord(page, wordInspector, output) : undefined;
  const pressFocusPolicy = refusePressFocus ? await observePressFocusRefusal(page) : undefined;
  const canvasFocusPolicy = refuseCanvasGesture
    ? await assertCanvasGestureTextAdmission(page)
    : undefined;
  const input = editor === "gridcraft" ? await exerciseGrid(page) : undefined;
  const result = await page.evaluate(() => ({
    states: window.probe.states,
    exports: window.probe.exports,
    writes: window.probe.writes,
    accepted: window.probe.accepted,
    version: window.probe.version,
  }));
  assert.equal(result.exports.length, formats.length + (editor !== "deckcraft" ? 1 : 0));
  if (editor === "wordcraft") {
    assert.ok(result.writes >= 2);
    assert.equal(result.version, result.writes + 1);
  } else {
    assert.equal(result.writes, editor === "gridcraft" ? 2 : 0);
    assert.equal(result.version, editor === "gridcraft" ? 3 : 1);
  }
  if (editor === "deckcraft") assert.equal(result.accepted, seed.toString("base64"));
  else assert.notEqual(result.accepted, seed.toString("base64"));
  for (const [index, file] of result.exports.entries()) {
    const b = Buffer.from(file.data, "base64");
    assert.equal(sha(b), file.sha256);
    assert.ok(
      file.format === "pdf"
        ? b.subarray(0, 5).toString() === "%PDF-"
        : b[0] === 0x50 && b[1] === 0x4b,
    );
    if (editor !== "deckcraft")
      assert.equal(file.data, index === 0 ? seed.toString("base64") : result.accepted);
    await writeFile(resolve(output, `accepted-${index + 1}.${file.format}`), b);
    if (editor === "wordcraft") {
      assert.equal(file.format, "docx");
      assert.equal(inspectWord(b, wordInspector), index === 0 ? "" : "Word browser\nSecond line");
    }
    if (editor === "gridcraft" && index > 0)
      assertGridExport(resolve(output, `accepted-${index + 1}.${file.format}`));
    delete file.data;
    file.bytes = b.length;
  }
  if (word)
    word.reopen = await reopenWord(
      page,
      ({ version, data }) => {
        files.set("/config.json", [
          Buffer.from(JSON.stringify({ ...config, version, seed: data })),
          "application/json",
        ]);
      },
      wordInspector,
      output,
    );
  await assert.rejects(
    page.evaluate((f) => window.probe.share(f), editor === "deckcraft" ? "xlsx" : "pdf"),
  );
  const reopened =
    editor === "gridcraft"
      ? await reopenGrid(
          page,
          (retained) => {
            files.set("/config.json", [
              Buffer.from(
                JSON.stringify({ ...config, seed: retained.data, version: retained.version }),
              ),
              "application/json",
            ]);
          },
          output,
        )
      : undefined;
  await page.screenshot({ path: resolve(output, "opened.png") });
  await page.evaluate(() => window.probe.retire());
  await assert.rejects(page.evaluate((f) => window.probe.share(f), formats[0]));
  assert.deepEqual(errors, []);
  assert.deepEqual(consoleErrors, []);
  receipt = {
    status: "PASS",
    editor,
    scope:
      "Actual generated MCP bootstrap + pinned engine + shared session/export, synthetic Core read/CAS port; no device or certified-agent claim.",
    sourceHtml: { bytes: Buffer.byteLength(html), sha256: sha(html) },
    module: { bytes: module.length, sha256: sha(module) },
    wasm: { bytes: wasm.length, sha256: sha(wasm) },
    seed: { bytes: seed.length, sha256: sha(seed) },
    fixtureBundleSha256: sha(bundled.outputFiles[0].contents),
    ...result,
    ...(word ? { word } : {}),
    ...(pressFocusPolicy ? { pressFocusPolicy } : {}),
    ...(canvasFocusPolicy ? { canvasFocusPolicy } : {}),
    input,
    reopened,
    requests,
    errors,
    consoleErrors,
  };
} catch (error) {
  receipt = { ...receipt, error: String(error), requests, errors, consoleErrors };
  throw error;
} finally {
  try {
    await browser?.close();
  } catch (e) {
    cleanupErrors.push(String(e));
  }
  server.closeAllConnections();
  if (server.listening) await new Promise((done) => server.close(done));
  await writeFile(
    resolve(output, "result.json"),
    JSON.stringify({ ...receipt, cleanupErrors }, null, 2) + "\n",
  );
}
