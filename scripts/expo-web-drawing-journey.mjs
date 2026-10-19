import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

// Observe real host-to-guest read responses. This never calls an app API, mutates
// a scene, injects storage/session state, or replaces the MCP transport.
async function observeCanonicalReads(page) {
  await page.addInitScript(() => {
    if (window === window.top) return;
    const observation = { manifest: null, chunks: [] };
    window.__zoenDrawingReadEvidence = observation;
    window.addEventListener("message", (event) => {
      if (event.source !== window.parent) return;
      let message = event.data;
      if (typeof message === "string") {
        if (message.length > 1 << 20) return;
        try { message = JSON.parse(message); } catch { return; }
      }
      const value = message?.result?.structuredContent;
      if (!value || typeof value !== "object") return;
      if (typeof value.transferId === "string" && /^[a-f0-9]{64}$/.test(value.sha256) &&
          Number.isSafeInteger(value.bytes) && value.bytes > 0 && value.bytes <= 16 << 20 &&
          value.chunkBytes === 196_608) {
        observation.manifest = value;
        observation.chunks = [];
      } else if (observation.manifest && Number.isSafeInteger(value.index) &&
          value.index === observation.chunks.length && value.index < 86 &&
          typeof value.data === "string" && value.data.length <= 262_144) {
        observation.chunks.push(value.data);
      }
    });
  });
}

async function readRenderedScene(frame) {
  const observed = await frame.locator("body").evaluate(() => window.__zoenDrawingReadEvidence);
  assert.ok(observed?.manifest, "the actual MCP guest must receive a canonical read manifest");
  const bytes = Buffer.concat(observed.chunks.map((chunk) => Buffer.from(chunk, "base64")));
  assert.equal(bytes.length, observed.manifest.bytes);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), observed.manifest.sha256);
  return { bytes, manifest: observed.manifest, scene: JSON.parse(bytes.toString("utf8")) };
}

export async function acceptDrawingJourney({
  page, ui, origin, space, tag, privateRoot, peerHome, cli, screenshot, step,
}) {
  const title = `Expo drawing ${tag}`;
  const label = `Shared canvas ${tag}`;
  const id = (await cli(peerHome, "app", space, "install", "excalidraw", JSON.stringify({ title }))).trim();
  assert.match(id, /^it_[a-z0-9]+$/);
  await observeCanonicalReads(page);
  await page.goto(`${origin}/miniapp/${id}`, { waitUntil: "domcontentloaded" });
  const selector = `iframe[title=${JSON.stringify(title)}]`;
  const frame = page.frameLocator(selector);
  const editor = frame.locator('main[data-canonical-editor="true"]');
  await editor.waitFor();
  assert.equal(await page.locator(selector).getAttribute("sandbox"), "allow-scripts");
  const initial = await readRenderedScene(frame);
  assert.deepEqual(initial.scene.elements, []);
  step("canonical-excalidraw-opens-in-strict-browser-sandbox");

  const canvas = frame.locator("canvas.excalidraw__canvas.interactive");
  await canvas.waitFor();
  await canvas.scrollIntoViewIfNeeded();
  await canvas.click({ position: { x: 240, y: 160 } });
  await page.keyboard.press("r");
  const bounds = await canvas.boundingBox();
  assert.ok(bounds && bounds.width > 450 && bounds.height > 320, "real drawing canvas is visible");
  await page.mouse.move(bounds.x + 260, bounds.y + 160);
  await page.mouse.down();
  await page.mouse.move(bounds.x + 380, bounds.y + 230, { steps: 8 });
  await page.mouse.up();
  await page.keyboard.press("t");
  await canvas.click({ position: { x: 260, y: 290 } });
  const text = frame.locator("textarea.excalidraw-wysiwyg");
  await text.waitFor();
  await text.fill(label);
  await text.press("Escape");
  await page.getByTestId("drawing-save-status").filter({ hasText: ui.drawingSaved }).waitFor();

  const currentPath = join(privateRoot, `peer-drawing-${tag}.excalidraw`);
  let peerBytes, peerScene;
  // The host's durable local ACK can precede the peer's encrypted sync. Require
  // the peer's committed text rather than assuming a fixed network delay.
  for (let attempt = 0; attempt < 3; attempt++) {
    await cli(peerHome, "file", "get", id, "--out", currentPath);
    peerBytes = await readFile(currentPath);
    peerScene = JSON.parse(peerBytes.toString("utf8"));
    if (peerScene.elements.some((element) => !element.isDeleted && element.text === label)) break;
  }
  assert.ok(peerScene.elements.some((element) => !element.isDeleted && element.type === "rectangle"));
  assert.ok(peerScene.elements.some((element) => !element.isDeleted && element.type === "text" && element.text === label));
  step("real-canvas-and-text-edits-reach-encrypted-native-peer-document");

  const reloaded = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(reloaded?.status(), 200);
  await editor.waitFor();
  const restored = await readRenderedScene(frame);
  assert.deepEqual(restored.bytes, peerBytes, "cold browser read must equal native peer bytes in full");
  assert.ok(restored.manifest.version > initial.manifest.version);
  const originalPath = join(privateRoot, `peer-drawing-original-${tag}.excalidraw`);
  await cli(peerHome, "file", "get", id, "--version", "1", "--out", originalPath);
  assert.deepEqual(await readFile(originalPath), initial.bytes, "original encrypted version is retained");
  await screenshot("10-excalidraw-canonical-cold-reload.png");
  step("cold-canonical-drawing-reload-matches-peer-bytes-and-retains-original-version", {
    bytes: peerBytes.length,
    sha256: restored.manifest.sha256,
    version: restored.manifest.version,
  });
}
