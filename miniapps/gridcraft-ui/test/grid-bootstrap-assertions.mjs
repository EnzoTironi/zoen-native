import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

function cells(path) {
  return JSON.parse(
    execFileSync(
      "python3",
      [fileURLToPath(new URL("./xlsx_export.py", import.meta.url)), "cells", path],
      { encoding: "utf8", timeout: 10000 },
    ),
  );
}

/** Actual engine keyboard + shared autosave consumer; synthetic Core CAS, not
 * gesture admission, native keyboard, relay persistence, or agent authority. */
export async function exerciseGrid(page) {
  const frame = page.frames().find((f) => f !== page.mainFrame());
  assert.ok(frame);
  await frame.locator("#gridcraft_canvas").focus();
  await page.evaluate(() => window.probe.holdNextWrite());
  await page.keyboard.type("7");
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.probe.writeHeld, undefined, { timeout: 8000 });
  await page.keyboard.type("9");
  await page.keyboard.press("Enter");
  await frame.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  );
  assert.equal(await page.evaluate(() => window.probe.writeHeld), true);
  await page.evaluate(() => window.probe.releaseWrite());
  await page.evaluate(() => window.probe.flush());
  const saved = await page.evaluate(() => ({
    writes: window.probe.writes,
    version: window.probe.version,
  }));
  assert.equal(saved.writes, 2, "Newer input saves separately from immutable first proposal");
  assert.equal(saved.version, 3);
  await page.evaluate(() => window.probe.share("xlsx"));
  return { delayedAckWrites: saved.writes, version: saved.version };
}

export function assertGridExport(path) {
  const actual = cells(path);
  assert.equal(Number(actual.A1?.value), 7);
  assert.equal(
    Number(actual.A2?.value),
    9,
    "Actual exported newer draft survives the original ACK",
  );
  return actual;
}

export async function reopenGrid(page, replaceConfig, output) {
  await page.evaluate(() => window.probe.close());
  const retained = await page.evaluate(() => ({
    version: window.probe.version,
    data: window.probe.accepted,
  }));
  assert.equal(retained.version, 3);
  replaceConfig(retained);
  await page.reload({ waitUntil: "load" });
  await page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 25000 });
  assert.equal(await page.evaluate(() => window.probe.version), retained.version);
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  await page.evaluate(() => window.probe.share("xlsx"));
  const file = await page.evaluate(() => window.probe.exports[0]);
  assert.equal(file.data, retained.data, "Fresh renderer reopens the exact accepted bytes");
  const path = resolve(output, "reopened.xlsx");
  await writeFile(path, Buffer.from(file.data, "base64"));
  return {
    version: retained.version,
    sha256: file.sha256,
    writesAfterReopen: 0,
    cells: assertGridExport(path),
  };
}
