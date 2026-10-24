import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "../../../apps/zoen/node_modules/esbuild/lib/main.js";
import { inspectWord } from "../../gridcraft-ui/test/word-bootstrap-assertions.mjs";
import { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";
const [packageDir, inspector, output, playwrightPath, baseline] = process.argv.slice(2);
assert.ok([packageDir, inspector, output, playwrightPath].every(isAbsolute));
await mkdir(output, { recursive: true });
const root = resolve(new URL("../../..", import.meta.url).pathname);
const sha = (b) => createHash("sha256").update(b).digest("hex");
const module = await readFile(resolve(packageDir, "wordcraft.js"));
const wasm = await readFile(resolve(packageDir, "wordcraft_bg.wasm"));
const html = await readFile(resolve(packageDir, "wordcraft.html"), "utf8");
const seed = await readFile(resolve(root, "crates/miniapps/resources/wordcraft-empty.docx"));
const revision = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
const asset = (b, suffix, mime) => ({
  url: `https://code.example.test/wordcraft/${revision}/${sha(b)}.${suffix}`,
  bytes: b.length,
  sha256: sha(b),
  mime,
});
const config = {
  html,
  seed: seed.toString("base64"),
  runtime: {
    schema: 1,
    engine: "wordcraft",
    revision,
    abi: "zoen-host-v1",
    module: asset(module, "js", "text/javascript"),
    wasm: asset(wasm, "wasm", "application/wasm"),
  },
};
const source = baseline
  ? execFileSync(
      "git",
      [
        "show",
        "327ab950c5c8e66ff26f576338cba1b9e9ea7bd2:apps/zoen/src/miniapps/deckcraft-session.ts",
      ],
      { cwd: root, encoding: "utf8" },
    )
  : undefined;
const bundled = await build({
  absWorkingDir: root,
  entryPoints: ["miniapps/wordcraft-ui/test/own-write-refresh.fixture.tsx"],
  bundle: true,
  write: false,
  format: "iife",
  globalName: "WordOwnWrite",
  platform: "browser",
  target: ["safari17"],
  jsx: "automatic",
  preserveSymlinks: true,
  nodePaths: [resolve(root, "apps/zoen/node_modules")],
  define: { "process.env.NODE_ENV": '"production"', "process.env.EXPO_OS": '"web"' },
  plugins: source
    ? [
        {
          name: "immutable-baseline-session",
          setup(build) {
            build.onLoad({ filter: /deckcraft-session\.ts$/ }, (args) => {
              assert.equal(args.path, resolve(root, "apps/zoen/src/miniapps/deckcraft-session.ts"));
              return {
                contents: source,
                loader: "ts",
                resolveDir: resolve(root, "apps/zoen/src/miniapps"),
              };
            });
          },
        },
      ]
    : [],
});
let browser, page;
const errors = [],
  consoleErrors = [],
  cleanupErrors = [];
let receipt = {
  status: "FAIL",
  baseline: !!baseline,
  scope:
    "Actual React MiniappFrame.web, ClientStore, official SDK and existing Word WASM; synthetic native Core and share sink. No iOS keyboard/driver or certified-agent claim.",
  module: { bytes: module.length, sha256: sha(module) },
  wasm: { bytes: wasm.length, sha256: sha(wasm) },
  htmlSha256: sha(Buffer.from(html)),
  bundleSha256: sha(bundled.outputFiles[0].contents),
  baselineSessionSha256: source ? sha(Buffer.from(source)) : undefined,
};
try {
  const pw = await import(pathToFileURL(playwrightPath).href);
  const webkit = process.env.ZOEN_PROBE_WEBKIT === "1";
  receipt.browser = webkit ? "webkit" : "chromium";
  browser = await (webkit ? pw.webkit : pw.chromium).launch(
    webkit
      ? { headless: true }
      : {
          executablePath: await findBrowserExecutable(),
          headless: true,
          args: [
            "--enable-unsafe-swiftshader",
            "--use-angle=swiftshader",
            "--disable-background-networking",
            "--disable-component-update",
          ],
        },
  );
  page = await browser.newPage({
    ...(webkit ? pw.devices["iPhone 13"] : {}),
    viewport: { width: 402, height: 874 },
    deviceScaleFactor: 1,
    hasTouch: true,
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (e) => {
    if (e.type() === "error") consoleErrors.push(e.text());
  });
  const routes = new Map([
    [config.runtime.module.url, [module, "text/javascript"]],
    [config.runtime.wasm.url, [wasm, "application/wasm"]],
  ]);
  await page.route("https://code.example.test/**", async (route) => {
    const f = routes.get(route.request().url());
    assert.ok(f);
    await route.fulfill({
      status: 200,
      body: f[0],
      headers: {
        "Content-Type": f[1],
        "Content-Length": String(f[0].length),
        "Access-Control-Allow-Origin": "*",
      },
    });
  });
  await page.setContent(
    '<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0}#root{height:100vh}</style><div id="root"></div>',
  );
  await page.addScriptTag({ content: bundled.outputFiles[0].text });
  await page.evaluate((config) => window.WordOwnWrite.startProbe(config), config);
  await page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 30000 });
  const frame = page.frames().find((f) => f !== page.mainFrame());
  assert.ok(frame);
  const bounds = await frame.locator("#wordcraft_canvas").boundingBox();
  assert.ok(bounds);
  await page.touchscreen.tap(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await frame.waitForFunction(
    () => document.activeElement?.matches("input[type=text]"),
    undefined,
    { timeout: 10000 },
  );
  await page.evaluate(() => window.probe.markInput());
  await page.screenshot({ path: resolve(output, "before-single-type.png") });
  const marker = "QA597 native rich";
  await page.keyboard.type(marker, { delay: 50 });
  await page.waitForFunction(() => window.probe.writeHeld, undefined, { timeout: 10000 });
  const input = await frame.evaluate(() => ({
    activeRole: document.activeElement?.tagName,
    focused: document.activeElement?.matches("input[type=text]"),
    docFocus: document.hasFocus(),
  }));
  const observed = await page.evaluate(() => ({
    writes: window.probe.writes,
    version: window.probe.version,
    notifications: window.probe.notifications,
    blocked: window.probe.states.map((s) => s.blocked),
    inert: document.querySelector("iframe").inert,
  }));
  receipt.inputBeforeAck = {
    ...input,
    ...observed,
    typeCommands: 1,
    requestedCharacters: marker.length,
  };
  await page.screenshot({ path: resolve(output, "after-single-type-before-ack.png") });
  await page.evaluate(() => window.probe.finish());
  const done = await page.evaluate(() => ({
    version: window.probe.version,
    writes: window.probe.writes,
    accepted: window.probe.accepted,
    exports: window.probe.exports,
    errors: window.probe.errors,
  }));
  assert.equal(done.exports.length, 1);
  const bytes = Buffer.from(done.exports[0].data, "base64");
  assert.equal(sha(bytes), done.exports[0].sha256);
  await writeFile(resolve(output, "accepted.docx"), bytes);
  const text = inspectWord(bytes, inspector);
  receipt.exportedText = text;
  receipt.finalHead = { version: done.version, sha256: sha(bytes) };
  receipt.writes = done.writes;
  receipt.hostErrors = done.errors;
  assert.equal(input.focused, true, "own refresh must not blur the real Word input before ACK");
  assert.equal(observed.inert, false);
  assert.equal(observed.blocked.some(Boolean), false);
  assert.equal(text, marker, "one trusted type command must reach the actual DOCX engine in full");
  assert.equal(done.version, 3);
  assert.equal(done.writes, 2);
  assert.equal(done.exports[0].data, done.accepted);
  assert.deepEqual(done.errors, []);
  assert.deepEqual(errors, []);
  assert.deepEqual(consoleErrors, []);
  receipt.status = "PASS";
} catch (error) {
  receipt.error = String(error);
  throw error;
} finally {
  if (page)
    await page.evaluate(() => window.probe?.retire()).catch((e) => cleanupErrors.push(String(e)));
  await browser?.close().catch((e) => cleanupErrors.push(String(e)));
  await writeFile(
    resolve(output, "result.json"),
    JSON.stringify({ ...receipt, errors, consoleErrors, cleanupErrors }, null, 2) + "\n",
  );
}
