// Real rendered PDF App and public SDK/engine transport. Core CAS is synthetic.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { openManagedBrowser, sha } from "../../shared-tests/managed-browser.mjs";
const [modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath] = process.argv.slice(2);
const host = await openManagedBrowser({
  editor: "pdfcraft", modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath,
});
let result = { status: "FAIL" };
try {
  const page = host.page;
  const child = page.frames().find((frame) => frame.parentFrame());
  assert.ok(child);
  await child.evaluate(() => {
    window.pdfTouchTrace = [];
    for (const type of ["touchstart", "touchend", "pointerdown", "pointerup", "focusin"])
      document.addEventListener(type, (event) => window.pdfTouchTrace.push({
        type, trusted: event.isTrusted, target: event.target?.tagName,
        phase: event.eventPhase, active: document.activeElement?.tagName,
      }), { capture: true, passive: true });
  });
  const frame = () => child.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  );
  const exported = async (name) => {
    await page.evaluate(() => window.probe.share("pdf"));
    const value = await page.evaluate(() => window.probe.exports.at(-1));
    assert.equal(value.format, "pdf");
    const bytes = Buffer.from(value.data, "base64");
    assert.equal(sha(bytes), value.sha256);
    await writeFile(resolve(output, `${name}.pdf`), bytes);
    return value;
  };
  const initial = await exported("baseline");
  assert.equal(initial.data, host.seed.toString("base64"));
  await page.screenshot({ path: resolve(output, "initial.png") });
  // Geometry comes from the original two-page A4 fixture rendered by this App.
  // The native toolbar is 48pt high; this point is inside its first paragraph.
  await page.touchscreen.tap(28, 24);
  await frame();
  await page.touchscreen.tap(124, 106);
  await page.screenshot({ path: resolve(output, "first-touch.png") });
  await child.waitForFunction(() => document.activeElement?.tagName === "INPUT", undefined,
    { timeout: 3000 });
  await page.keyboard.type(" browser touch");
  await page.evaluate(() => window.probe.flush());
  assert.equal(await page.evaluate(() => window.probe.version), 2);
  assert.equal(await page.evaluate(() => window.probe.writes), 1);
  const typed = await exported("typed");
  assert.notEqual(typed.sha256, initial.sha256);
  await page.screenshot({ path: resolve(output, "typed.png") });
  await page.touchscreen.tap(166, 24);
  await frame();
  await page.evaluate(() => window.probe.flush());
  assert.equal(await page.evaluate(() => window.probe.version), 3);
  const undone = await exported("undone");
  assert.notEqual(undone.sha256, typed.sha256);
  await page.touchscreen.tap(212, 24);
  await frame();
  await page.evaluate(() => window.probe.flush());
  assert.equal(await page.evaluate(() => window.probe.version), 4);
  assert.equal(await page.evaluate(() => window.probe.writes), 3);
  const redone = await exported("redone");
  await page.screenshot({ path: resolve(output, "redone.png") });
  const trace = await child.evaluate(() => window.pdfTouchTrace);
  assert.ok(trace.some((event) => event.type === "touchend" && event.trusted));
  await page.evaluate(() => window.probe.close());
  await host.reopen(redone.data, 4);
  assert.equal(await page.evaluate(() => window.probe.version), 4);
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  const reopened = await exported("reopened");
  assert.equal(reopened.data, redone.data);
  assert.equal(reopened.sha256, redone.sha256);
  await page.screenshot({ path: resolve(output, "reopened.png") });
  await page.evaluate(() => window.probe.retire());
  await assert.rejects(page.evaluate(() => window.probe.share("pdf")));
  assert.deepEqual(host.errors, []);
  assert.deepEqual(host.consoleErrors, []);
  result = {
    status: "PASS", ...host.inputs, trace,
    proof: "Actual trusted touch/input, real engine Undo/Redo, SDK save ACKs and exact chosen-version export/cold reopen; synthetic Core CAS only. Independent PDF semantic verification is separate; no iPhone or agent authorization claim.",
    exported: Object.fromEntries(Object.entries({ initial, typed, undone, redone, reopened })
      .map(([name, value]) => [name, { sha256: value.sha256, bytes: Buffer.from(value.data, "base64").length }])),
    finalVersion: 4, writesBeforeReopen: 3, coldWrites: 0,
  };
} catch (error) {
  result.error = String(error);
  const child = host.page.frames().find((frame) => frame.parentFrame());
  result.trace = await child?.evaluate(() => window.pdfTouchTrace).catch(() => []);
  await host.page.screenshot({ path: resolve(output, "failure.png") }).catch(() => {});
  throw error;
} finally {
  await host.close();
  await writeFile(resolve(output, "result.json"), JSON.stringify({
    ...result, requests: host.requests, errors: host.errors,
    consoleErrors: host.consoleErrors, cleanupErrors: host.cleanupErrors,
  }, null, 2) + "\n");
}
