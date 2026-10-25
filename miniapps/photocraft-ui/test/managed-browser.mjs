import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, isAbsolute } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";
import { build } from "../../../apps/zoen/node_modules/esbuild/lib/main.js";
import { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";
import { pauseOnCompletedTouch } from "./input-flush-assertions.mjs";
import { rotateFromHumanMenu } from "./menu-rotation.mjs";
import { reviewExternalPhoto } from "./live-review.mjs";
const editor = "photocraft";
const [modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath] = process.argv.slice(2);
if (
  ![modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath].every(
    (p) => p && isAbsolute(p),
  )
)
  throw Error(
    "Usage: managed-browser.mjs ABS_MODULE ABS_WASM ABS_HTML ABS_SEED ABS_NEW_OUTPUT ABS_PLAYWRIGHT",
  );
await mkdir(output, { recursive: true });
const sha = (b) => createHash("sha256").update(b).digest("hex");
const pixels = (path) =>
  JSON.parse(
    execFileSync(
      process.env.PHOTO_PYTHON ?? "python3",
      [fileURLToPath(new URL("./png-probe.py", import.meta.url)), path],
      { encoding: "utf8", timeout: 10000 },
    ),
  );
const module = await readFile(modulePath),
  wasm = await readFile(wasmPath);
const html = await readFile(htmlPath, "utf8"),
  seed = await readFile(seedPath);
assert.ok(module.length <= 512 * 1024 && wasm.length <= 48 * 1024 * 1024);
const revision = "9b9643764f3fb77d9f0e48d7510983ae50621e11";
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
        '<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0}iframe{width:100vw;height:100vh;border:0}</style><iframe sandbox="allow-scripts"></iframe><script type="module">import {startProbe} from "/probe.js";await startProbe(await(await fetch("/config.json")).json());</script>',
      ),
      "text/html",
    ],
  ],
]);
const requests = [],
  errors = [],
  consoleErrors = [],
  cleanupErrors = [];
let browser,
  receipt = { status: "FAIL" };
try {
  const { chromium, webkit, devices } = await import(pathToFileURL(playwrightPath).href);
  const isWebkit = process.env.ZOEN_PROBE_WEBKIT === "1";
  browser = await (isWebkit ? webkit : chromium).launch(
    isWebkit
      ? { headless: true, timeout: 30000 }
      : {
          executablePath: await findBrowserExecutable(),
          headless: true,
          timeout: 30000,
          args: [
            "--enable-unsafe-swiftshader",
            "--use-angle=swiftshader",
            "--disable-background-networking",
            "--disable-component-update",
          ],
        },
  );
  receipt.browser = { name: browser.browserType().name(), version: browser.version() };
  const page = await browser.newPage({
    ...(isWebkit ? devices["iPhone 13"] : {}),
    viewport:
      process.env.PHOTO_BASELINE_DESKTOP === "1"
        ? { width: 1100, height: 800 }
        : { width: 390, height: 844 },
    deviceScaleFactor: 1,
    hasTouch: true,
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (e) => {
    if (e.type() === "error") consoleErrors.push(e.text());
  });
  // Fulfill the public fixture origin in memory: no local server or network service.
  await page.route("**/*", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    // WebKit also routes the already verified engine's in-memory module URL.
    if (url.protocol === "blob:") {
      await route.continue();
      return;
    }
    requests.push({ method: request.method(), path: url.pathname, origin: url.origin });
    const file = url.origin === "https://photocraft-fixture.invalid" && files.get(url.pathname);
    if (request.method() !== "GET" || !file) {
      await route.fulfill({ status: 404, body: "Unknown fixture input" });
      return;
    }
    await route.fulfill({ status: 200, contentType: file[1], body: Buffer.from(file[0]), headers: { "Cache-Control": "no-store" } });
  });
  await page.goto("https://photocraft-fixture.invalid/", { waitUntil: "load" });
  await page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 25000 });
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  const formats = ["pcraft", "png"];
  for (const format of formats) await page.evaluate((f) => window.probe.share(f), format);
  await page.screenshot({ path: resolve(output, "initial.png") });
  const result = await page.evaluate(() => ({
    states: window.probe.states,
    exports: window.probe.exports,
    writes: window.probe.writes,
    accepted: window.probe.accepted,
    version: window.probe.version,
  }));
  assert.equal(result.writes, 0);
  assert.equal(result.version, 1);
  assert.equal(result.accepted, seed.toString("base64"));
  for (const file of result.exports) {
    const bytes = Buffer.from(file.data, "base64");
    assert.equal(sha(bytes), file.sha256);
    if (file.format === "pcraft") assert.equal(file.data, seed.toString("base64"));
    else assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
    await writeFile(resolve(output, `baseline.${file.format}`), bytes);
    delete file.data;
    file.bytes = bytes.length;
  }
  const originalPixels = pixels(resolve(output, "baseline.png"));
  assert.deepEqual(originalPixels, {
    width: 640,
    height: 480,
    blackPixels: 1800,
    newBlackPixels: 0,
    white: [255, 255, 255, 255],
    black: [0, 0, 0, 255],
    green: [0, 255, 0, 255],
  });
  await page.evaluate(() => window.probe.holdNextWrite());
  const touchPause = await pauseOnCompletedTouch(page, 180, 430);
  await page.waitForFunction(() => window.probe.writeHeld, undefined, { timeout: 10000 });
  await page.touchscreen.tap(230, 500);
  const frame = page.frames().find((f) => f !== page.mainFrame());
  assert.ok(frame);
  await frame.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  );
  assert.equal(await page.evaluate(() => window.probe.writeHeld), true);
  await page.evaluate(() => window.probe.releaseWrite());
  await page.evaluate(() => window.probe.flush());
  for (const format of formats) await page.evaluate((f) => window.probe.share(f), format);
  const edited = await page.evaluate(() => ({
    writes: window.probe.writes,
    version: window.probe.version,
    accepted: window.probe.accepted,
    exports: window.probe.exports.slice(2),
  }));
  assert.equal(edited.writes, 2);
  assert.equal(edited.version, 3);
  assert.notEqual(edited.accepted, seed.toString("base64"));
  for (const file of edited.exports) {
    const bytes = Buffer.from(file.data, "base64");
    assert.equal(sha(bytes), file.sha256);
    if (file.format === "pcraft") assert.equal(file.data, edited.accepted);
    await writeFile(resolve(output, `edited.${file.format}`), bytes);
    delete file.data;
    file.bytes = bytes.length;
  }
  const editedPixels = pixels(resolve(output, "edited.png"));
  assert.ok(editedPixels.newBlackPixels > 10, "trusted touches must paint real pixels");
  assert.deepEqual(editedPixels.white, originalPixels.white);
  assert.deepEqual(editedPixels.black, originalPixels.black);
  assert.deepEqual(editedPixels.green, originalPixels.green);
  const menuRotation =
    process.env.PHOTO_MENU_ROTATION === "1"
      ? await rotateFromHumanMenu(page, output, edited, pixels)
      : undefined;
  assert.ok(!(process.env.PHOTO_LIVE_REVIEW === "1" && menuRotation), "Run live review and menu suites separately");
  const liveReview = process.env.PHOTO_LIVE_REVIEW === "1"
    ? await reviewExternalPhoto(page, output, edited, pixels)
    : undefined;
  const finalDocument = liveReview ?? menuRotation ?? edited;
  const finalPixels = liveReview?.pixels ?? menuRotation?.pixels ?? editedPixels;
  await page.setViewportSize({ width: 844, height: 390 });
  await page.screenshot({ path: resolve(output, "landscape.png") });
  assert.equal(
    await page.evaluate(() => window.probe.writes),
    finalDocument.writes,
    "viewport rotation must not save a document edit",
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => window.probe.close());
  files.set("/config.json", [
    Buffer.from(
      JSON.stringify({ ...config, seed: finalDocument.accepted, version: finalDocument.version }),
    ),
    "application/json",
  ]);
  await page.reload({ waitUntil: "load" });
  await page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 25000 });
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  assert.equal(await page.evaluate(() => window.probe.version), finalDocument.version);
  for (const format of formats) await page.evaluate((f) => window.probe.share(f), format);
  const reopened = await page.evaluate(() => window.probe.exports);
  for (const file of reopened) {
    const bytes = Buffer.from(file.data, "base64");
    assert.equal(sha(bytes), file.sha256);
    if (file.format === "pcraft")
      assert.equal(
        file.data,
        finalDocument.accepted,
        "fresh renderer reopens exact accepted bytes",
      );
    await writeFile(resolve(output, `reopened.${file.format}`), bytes);
  }
  assert.deepEqual(pixels(resolve(output, "reopened.png")), finalPixels);
  await page.screenshot({ path: resolve(output, "opened.png") });
  await page.evaluate(() => window.probe.retire());
  await assert.rejects(page.evaluate((f) => window.probe.share(f), formats[0]));
  assert.deepEqual(errors, []);
  assert.deepEqual(consoleErrors, []);
  receipt = {
    status: "PASS",
    browser: receipt.browser,
    editor,
    scope:
      "Actual generated MCP bootstrap + pinned engine + shared session/export, synthetic Core read/CAS port; no device or certified-agent claim.",
    sourceHtml: { bytes: Buffer.byteLength(html), sha256: sha(html) },
    module: { bytes: module.length, sha256: sha(module) },
    wasm: { bytes: wasm.length, sha256: sha(wasm) },
    seed: { bytes: seed.length, sha256: sha(seed) },
    fixtureBundleSha256: sha(bundled.outputFiles[0].contents),
    ...result,
    originalPixels,
    touchPause,
    editedPixels,
    edited,
    menuRotation,
    liveReview,
    coldReopen: {
      version: finalDocument.version,
      writes: 0,
      sha256: sha(Buffer.from(finalDocument.accepted, "base64")),
    },
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
  await writeFile(
    resolve(output, "result.json"),
    JSON.stringify({ ...receipt, cleanupErrors }, null, 2) + "\n",
  );
}
