// Create/edit a real canonical blank PDF, using the upstream touch controls.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { openManagedBrowser, sha } from "../../shared-tests/managed-browser.mjs";
const [modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath] = process.argv.slice(2);
const h = await openManagedBrowser({ editor: "pdfcraft", modulePath, wasmPath, htmlPath,
  seedPath, output, playwrightPath });
let result = { status: "FAIL" };
try {
  const page = h.page, frame = page.frames().find((f) => f.parentFrame());
  assert.ok(frame);
  const painted = () => frame.evaluate(async () => {
    // Let the actual upstream popup enter animation finish before selecting it.
    for (let n = 0; n < 8; n++) await new Promise((done) => requestAnimationFrame(done));
  });
  const exported = async (name) => {
    await page.evaluate(() => window.probe.share("pdf"));
    const value = await page.evaluate(() => ({ ...window.probe.exports.at(-1),
      version: window.probe.version, writes: window.probe.writes }));
    const bytes = Buffer.from(value.data, "base64");
    assert.equal(sha(bytes), value.sha256);
    await writeFile(resolve(output, `${name}.pdf`), bytes);
    return value;
  };
  const baseline = await exported("baseline");
  assert.equal(baseline.data, h.seed.toString("base64"));
  await painted();
  await page.touchscreen.tap(304, 24); // More tools in the retained 48pt bar.
  await painted();
  await page.screenshot({ path: resolve(output, "more.png") });
  await page.touchscreen.tap(240, 85); // Actual menu's second row: Add text.
  await painted();
  await page.touchscreen.tap(125, 130); // Empty A4 page at its normal fitted view.
  await frame.waitForFunction(() => document.activeElement?.tagName === "INPUT");
  await page.keyboard.type("Created in Zoen");
  await painted();
  await page.screenshot({ path: resolve(output, "draft.png") });
  await page.touchscreen.tap(216, 144); // Retained text box's checkmark.
  await painted();
  await page.evaluate(() => window.probe.flush());
  const edited = await exported("edited");
  assert.equal(edited.version, 2);
  assert.equal(edited.writes, 1);
  await page.touchscreen.tap(166, 24);
  await painted();
  await page.evaluate(() => window.probe.flush());
  const undone = await exported("undone");
  assert.equal(undone.version, 3);
  await page.touchscreen.tap(212, 24);
  await painted();
  await page.evaluate(() => window.probe.flush());
  const redone = await exported("redone");
  assert.equal(redone.version, 4);
  assert.equal(redone.writes, 3);
  await page.evaluate(() => window.probe.close());
  await h.reopen(redone.data, 4);
  const reopened = await exported("reopened");
  assert.equal(reopened.data, redone.data);
  assert.equal(reopened.writes, 0);
  assert.deepEqual(h.errors, []);
  assert.deepEqual(h.consoleErrors, []);
  result = { status: "PASS", ...h.inputs,
    proof: "Real blank native seed, upstream touch menu/text box/checkmark, native Undo/Redo and exact cold chosen export. Synthetic Core CAS port; no iPhone or inference claim.",
    exported: Object.fromEntries(Object.entries({ baseline, edited, undone, redone, reopened })
      .map(([name, x]) => [name, { sha256: x.sha256, version: x.version, writes: x.writes }])) };
} catch (error) {
  result.error = String(error);
  await h.page.screenshot({ path: resolve(output, "failure.png") }).catch(() => {});
  throw error;
} finally {
  await h.close();
  await writeFile(resolve(output, "result.json"), JSON.stringify({ ...result,
    errors: h.errors, consoleErrors: h.consoleErrors, cleanupErrors: h.cleanupErrors }, null, 2) + "\n");
}
