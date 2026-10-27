import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

// Inspect the actual exported writer bytes with the genuine pinned engine. The
// fixture scope is only this component test's context, never canonical identity.
export function inspectWord(bytes, inspector) {
  const digest = createHash("sha256").update(bytes).digest("hex");
  const metadata = Buffer.from(
    JSON.stringify({
      schema: 1,
      open: {
        scope: { artifactId: "bootstrap-fixture", actorId: "fixture-human", actorKind: "human" },
        head: { version: "1", sha256: digest },
      },
      command: null,
    }),
  );
  const prefix = Buffer.alloc(4);
  prefix.writeUInt32LE(metadata.length);
  const child = spawnSync(inspector, [], {
    input: Buffer.concat([prefix, metadata, bytes]),
    timeout: 15000,
    maxBuffer: 1024 * 1024,
    encoding: "buffer",
  });
  assert.equal(child.error, undefined, "genuine Word inspector must execute");
  assert.equal(child.status, 0, "genuine Word inspector must exit successfully");
  assert.ok(child.stdout.length >= 4);
  const length = child.stdout.readUInt32LE(0);
  assert.ok(length > 0 && length <= 65536);
  assert.equal(child.stdout.length, length + 4, "inspect returns no invented document bytes");
  const response = JSON.parse(child.stdout.subarray(4).toString());
  assert.equal(response.ok, true, "actual DOCX reader/profile must accept exported bytes");
  assert.equal(response.metadata.head.sha256, digest);
  assert.equal(typeof response.metadata.text, "string");
  return response.metadata.text;
}

export async function exerciseWord(page, inspector, output) {
  const frame = page.frames().find((f) => f !== page.mainFrame());
  assert.ok(frame, "actual MCP child frame must exist");
  const canvas = frame.locator("#wordcraft_canvas");
  await canvas.waitFor({ state: "visible" });
  const bounds = await canvas.boundingBox();
  assert.ok(bounds && bounds.width > 0 && bounds.height > 0);
  const before = await frame.evaluate(() =>
    [...document.querySelectorAll("input[type=text]")].map((input) => ({
      focused: document.activeElement === input,
      pointerEvents: getComputedStyle(input).pointerEvents,
      rect: input.getBoundingClientRect().toJSON(),
    })),
  );
  await page.screenshot({ path: resolve(output, "before-primary-touch.png") });
  // A real primary touchscreen tap on the observed live canvas, never DOM focus,
  // a synthetic click in a fabricated TextEdit, a value setter, or a CDP focus call.
  await page.touchscreen.tap(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await frame.waitForFunction(() =>
    [...document.querySelectorAll("input[type=text]")].some(
      (input) =>
        document.activeElement === input && getComputedStyle(input).pointerEvents === "auto",
    ),
  );
  const after = await frame.evaluate(() =>
    [...document.querySelectorAll("input[type=text]")].map((input) => ({
      focused: document.activeElement === input,
      pointerEvents: getComputedStyle(input).pointerEvents,
      rect: input.getBoundingClientRect().toJSON(),
    })),
  );
  assert.equal(after.filter((input) => input.focused).length, 1);
  await page.evaluate(() => window.probe.holdNextWrite());
  await page.keyboard.type("Word ");
  await page.waitForFunction(() => window.probe.writeHeld, undefined, { timeout: 8000 });
  // These are real browser keys while the original canonical save is waiting.
  await page.keyboard.type("browser");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Second line");
  assert.equal(await page.evaluate(() => window.probe.writeHeld), true);
  await page.evaluate(() => window.probe.releaseWrite());
  await page.evaluate(() => window.probe.flush());
  const saved = await page.evaluate(() => ({
    writes: window.probe.writes,
    version: window.probe.version,
  }));
  assert.ok(
    saved.writes >= 2,
    "newer input must be a later save, not overwrite the in-flight proposal",
  );
  assert.equal(saved.version, saved.writes + 1);
  await page.evaluate(() => window.probe.share("docx"));
  await writeFile(
    resolve(output, "actual-touch-observations.json"),
    JSON.stringify(
      {
        canvas: bounds,
        before,
        after,
        limitation:
          "Chrome DOM focus/editing only; no iOS keyboard or Rust authority claim. Prior DOM focus is recorded, not treated as gesture proof.",
      },
      null,
      2,
    ) + "\n",
  );
  return {
    before,
    after,
    delayedAckWrites: saved.writes,
    inspector: {
      sha256: createHash("sha256")
        .update(await readFile(inspector))
        .digest("hex"),
    },
  };
}

// Reload a fresh renderer from the synthetic port's exact accepted bytes/head.
// This proves the shared close/open consumer, not Rust or physical persistence.
export async function reopenWord(page, replaceConfig, inspector, output) {
  await page.evaluate(() => window.probe.close());
  const retained = await page.evaluate(() => ({
    version: window.probe.version,
    data: window.probe.accepted,
  }));
  replaceConfig(retained);
  await page.reload({ waitUntil: "load" });
  await page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 25000 });
  assert.equal(await page.evaluate(() => window.probe.version), retained.version);
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  await page.evaluate(() => window.probe.share("docx"));
  const file = await page.evaluate(() => window.probe.exports[0]);
  assert.equal(file.data, retained.data);
  const bytes = Buffer.from(file.data, "base64");
  assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
  await writeFile(resolve(output, "reopened.docx"), bytes);
  assert.equal(inspectWord(bytes, inspector), "Word browser\nSecond line");
  return {
    version: retained.version,
    sha256: file.sha256,
    bytes: bytes.length,
    writesAfterReopen: 0,
  };
}
