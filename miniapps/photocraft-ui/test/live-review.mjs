import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, isAbsolute } from "node:path";
import { fileURLToPath } from "node:url";

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** Real upstream worker project -> official MCP refresh -> real renderer Undo.
 * Canonical publication/CAS remain synthetic here; this is not agent certification. */
export async function reviewExternalPhoto(page, output, before, pixels) {
  const worker = process.env.PHOTO_REVIEW_WORKER;
  assert.ok(worker && isAbsolute(worker), "Provide the actual qualified native worker executable");
  const bytes = Buffer.from(before.accepted, "base64");
  const base = { version: String(before.version), sha256: sha(bytes) };
  const scope = { artifactId: "bootstrap-fixture", actorId: "fixture-agent", actorKind: "agent" };
  const request = Buffer.from(JSON.stringify({
    schema: 1, open: { scope, head: base },
    command: { mutation: { scope, base, mutationId: "external-photo-rotation" }, action: "rotate_clockwise" },
  }));
  const prefix = Buffer.alloc(4);
  prefix.writeUInt32LE(request.length);
  const response = execFileSync(worker, [], {
    input: Buffer.concat([prefix, request, bytes]), timeout: 30000, maxBuffer: 9 * 1024 * 1024,
  });
  const length = response.readUInt32LE();
  assert.ok(length > 0 && length <= 65536 && response.length > length + 4);
  const envelope = JSON.parse(response.subarray(4, length + 4).toString());
  assert.equal(envelope.ok, true);
  const project = response.subarray(length + 4);
  assert.equal(envelope.metadata.sha256, sha(project));
  assert.deepEqual(envelope.metadata.operation, { kind: "command", action: "rotate_clockwise" });
  assert.deepEqual(envelope.metadata.base, base);
  assert.deepEqual(envelope.metadata.scope, scope);
  await writeFile(resolve(output, "external-worker.pcraft"), project);
  await page.evaluate(({ data, version }) => window.probe.receiveCanonical(data, version), {
    data: project.toString("base64"), version: before.version + 1,
  });
  assert.deepEqual(await page.evaluate(() => window.probe.observedHead()), {
    itemId: "bootstrap-fixture", version: before.version + 1, sha256: sha(project),
  });
  const exportCurrent = async (name) => {
    const start = await page.evaluate(() => window.probe.exports.length);
    for (const format of ["pcraft", "png"]) await page.evaluate((f) => window.probe.share(f), format);
    const value = await page.evaluate((start) => ({
      writes: window.probe.writes, version: window.probe.version,
      accepted: window.probe.accepted, exports: window.probe.exports.slice(start),
    }), start);
    for (const file of value.exports) {
      const data = Buffer.from(file.data, "base64");
      assert.equal(sha(data), file.sha256);
      if (file.format === "pcraft") assert.equal(file.data, value.accepted);
      await writeFile(resolve(output, `${name}.${file.format}`), data);
      delete file.data;
      file.bytes = data.length;
    }
    return value;
  };
  const external = await exportCurrent("external");
  assert.equal(external.writes, before.writes, "review must never save received canonical bytes again");
  assert.equal(external.accepted, project.toString("base64"));
  const oracle = JSON.parse(execFileSync(process.env.PHOTO_PYTHON ?? "python3", [
    fileURLToPath(new URL("./png-probe.py", import.meta.url)), "rotate-clockwise",
    resolve(output, "edited.png"), resolve(output, "external.png"),
  ], { encoding: "utf8", timeout: 10000 }));
  assert.equal(oracle.allPixelsEqual, true);
  const frame = page.frames().find((value) => value !== page.mainFrame());
  assert.ok(frame);
  await page.screenshot({ path: resolve(output, "external-canvas.png") });
  // eframe's public keyboard mapping treats Ctrl as egui COMMAND on both platforms.
  // This trusted key sequence exercises the actual upstream shortcut and Undo stack.
  await page.keyboard.press("Control+z");
  await frame.evaluate(async () => {
    for (let n = 0; n < 12; n++) await new Promise((done) => requestAnimationFrame(done));
  });
  await page.evaluate(() => window.probe.flush());
  const undone = await exportCurrent("undo-external");
  assert.equal(undone.accepted, before.accepted, "real Undo must restore exact pre-agent project bytes");
  assert.equal(undone.writes, before.writes + 1, "Undo becomes a new canonical save CAS");
  assert.equal(undone.version, before.version + 2);
  assert.deepEqual(await readFile(resolve(output, "undo-external.png")), await readFile(resolve(output, "edited.png")));
  await page.screenshot({ path: resolve(output, "undo-external-canvas.png") });
  return { ...undone, pixels: pixels(resolve(output, "undo-external.png")), workerSha256: sha(await readFile(worker)), operation: envelope.metadata.operation, external, oracle };
}
