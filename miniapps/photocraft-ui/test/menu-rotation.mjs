import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Actual canvas menu taps: no session commands, document mutation or provider calls.
export async function rotateFromHumanMenu(page, output, before, pixels) {
  const frames = async () => {
    const frame = page.frames().find((f) => f !== page.mainFrame());
    assert.ok(frame);
    await frame.evaluate(async () => {
      for (let n = 0; n < 12; n += 1) await new Promise((done) => requestAnimationFrame(done));
    });
  };
  await page.touchscreen.tap(130, 16);
  await frames();
  await page.screenshot({ path: resolve(output, "image-menu.png") });
  await page.touchscreen.tap(205, 241);
  await frames();
  await page.screenshot({ path: resolve(output, "image-rotation-menu.png") });
  // Verified pinned phone popup: the clockwise row is below 180°, within
  // the submenu bounds (115..295, 271..437) at the fixed 390×844 viewport.
  await page.touchscreen.tap(195, 313);
  await frames();
  await page.screenshot({ path: resolve(output, "rotated-canvas.png") });
  await page.evaluate(() => window.probe.flush());
  const start = await page.evaluate(() => window.probe.exports.length);
  for (const format of ["pcraft", "png"]) await page.evaluate((f) => window.probe.share(f), format);
  const after = await page.evaluate(
    (start) => ({
      writes: window.probe.writes,
      version: window.probe.version,
      accepted: window.probe.accepted,
      exports: window.probe.exports.slice(start),
    }),
    start,
  );
  assert.equal(after.writes, before.writes + 1, "actual menu rotation must persist once");
  assert.equal(after.version, before.version + 1);
  assert.notEqual(after.accepted, before.accepted);
  for (const file of after.exports) {
    const bytes = Buffer.from(file.data, "base64");
    assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
    if (file.format === "pcraft") assert.equal(file.data, after.accepted);
    await writeFile(resolve(output, `rotated.${file.format}`), bytes);
    delete file.data;
    file.bytes = bytes.length;
  }
  const oracle = JSON.parse(
    execFileSync(
      process.env.PHOTO_PYTHON ?? "python3",
      [
        fileURLToPath(new URL("./png-probe.py", import.meta.url)),
        "rotate-clockwise",
        resolve(output, "edited.png"),
        resolve(output, "rotated.png"),
      ],
      { encoding: "utf8", timeout: 10000 },
    ),
  );
  assert.equal(oracle.allPixelsEqual, true);
  return { ...after, pixels: pixels(resolve(output, "rotated.png")), oracle };
}
