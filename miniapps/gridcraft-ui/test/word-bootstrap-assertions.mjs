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
  await page.keyboard.type("Word browser");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Second line");
  await page.evaluate(() => window.probe.flush());
  await page.waitForFunction(() => window.probe.writes === 1, undefined, { timeout: 8000 });
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
    inspector: {
      sha256: createHash("sha256")
        .update(await readFile(inspector))
        .digest("hex"),
    },
  };
}
