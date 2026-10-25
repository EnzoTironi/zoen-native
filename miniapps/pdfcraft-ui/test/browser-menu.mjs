// Real retained menu/input path at the host's actual offset; no native-device inference.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { openManagedBrowser } from "../../shared-tests/managed-browser.mjs";
const [
  modulePath,
  wasmPath,
  htmlPath,
  seedPath,
  output,
  playwrightPath,
  topArg = "118",
  scaleArg = "3",
] = process.argv.slice(2);
const top = Number(topArg),
  scale = Number(scaleArg);
assert.ok([0, 118].includes(top));
assert.ok([1, 3].includes(scale));
const host = await openManagedBrowser({
  editor: "pdfcraft",
  modulePath,
  wasmPath,
  htmlPath,
  seedPath,
  output,
  playwrightPath,
  deviceScaleFactor: scale,
  host: { theme: "dark", safeAreaInsets: { top, right: 0, bottom: top ? 34 : 0, left: 0 } },
});
let result = { status: "FAIL" };
try {
  const page = host.page,
    frame = page.frames().find((f) => f.parentFrame());
  assert.ok(frame);
  const painted = () =>
    frame.evaluate(async () => {
      for (let n = 0; n < 10; n++) await new Promise((done) => requestAnimationFrame(done));
    });
  await frame.evaluate(() => {
    window.inputEvents = [];
    for (const type of ["pointerdown", "pointermove", "pointerup", "touchstart", "touchend"])
      document.addEventListener(
        type,
        (e) =>
          window.inputEvents.push({
            type,
            trusted: e.isTrusted,
            x: e.clientX ?? e.changedTouches?.[0]?.clientX,
            y: e.clientY ?? e.changedTouches?.[0]?.clientY,
            pointerType: e.pointerType,
            target: e.target?.tagName,
          }),
        true,
      );
  });
  await painted();
  const geometry = await frame.evaluate(() => ({
    innerWidth,
    innerHeight,
    devicePixelRatio,
    canvas: [...document.querySelectorAll("canvas")].map((c) => ({
      id: c.id,
      width: c.width,
      height: c.height,
      rect: c.getBoundingClientRect().toJSON(),
    })),
  }));
  assert.equal(geometry.devicePixelRatio, scale);
  assert.equal(geometry.innerWidth, 402);
  assert.equal(geometry.innerHeight, 874);
  await page.screenshot({ path: resolve(output, "initial.png") });
  await page.touchscreen.tap(304, 24 + top); // One tap on the observed More control.
  await painted();
  await page.screenshot({ path: resolve(output, "more.png") });
  const afterMore = await page.evaluate(() => ({
    writes: window.probe.writes,
    version: window.probe.version,
  }));
  assert.deepEqual(afterMore, { writes: 0, version: 1 });
  await page.touchscreen.tap(240, 85 + top); // Actual upstream menu, second row: Add text.
  await painted();
  await page.touchscreen.tap(125, 130 + top); // Fitted blank A4 page.
  await painted();
  await page.screenshot({ path: resolve(output, "add-text.png") });
  const input = await frame.evaluate(() => ({
    activeTag: document.activeElement?.tagName,
    focused: document.hasFocus(),
    input: [...document.querySelectorAll("input")].map((x) => ({
      display: getComputedStyle(x).display,
      pointer: getComputedStyle(x).pointerEvents,
      focused: x === document.activeElement,
    })),
  }));
  assert.equal(
    input.activeTag,
    "INPUT",
    "real More → Add text → page must open the editor's text input",
  );
  assert.equal(input.focused, true);
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  assert.deepEqual(host.errors, []);
  assert.deepEqual(host.consoleErrors, []);
  result = {
    status: "PASS",
    ...host.inputs,
    geometry,
    top,
    scale,
    afterMore,
    input,
    proof:
      "Actual sealed PDF engine/HTML touch menu and text activation, same host offset/DPR; synthetic Core port. No iPhone, driver or interference attribution.",
  };
} catch (error) {
  result.error = String(error);
  await host.page.screenshot({ path: resolve(output, "failure.png") }).catch(() => {});
  throw error;
} finally {
  const frame = host.page.frames().find((f) => f.parentFrame());
  const events = await frame?.evaluate(() => window.inputEvents).catch(() => []);
  await host.close();
  await writeFile(
    resolve(output, "result.json"),
    JSON.stringify(
      {
        ...result,
        events,
        errors: host.errors,
        consoleErrors: host.consoleErrors,
        cleanupErrors: host.cleanupErrors,
      },
      null,
      2,
    ) + "\n",
  );
}
