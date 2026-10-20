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
  page, ui, browserAuthor, nativeAuthor, origin, space, tag, privateRoot, peerHome, cli, screenshot, step,
}) {
  const title = `Expo drawing ${tag}`;
  const label = `Shared canvas ${tag}`;
  // Discover the genuine earlier list/message; its copy may come from an old or
  // current CLI. Never require legacy text or globally unique message bodies.
  const historyBefore = await cli(peerHome, "read", space);
  const lists = (await cli(peerHome, "app", space, "view")).split("\n").filter(Boolean)
    .map((line) => line.split("\t"))
    .filter((fields) => fields[1] === "list" && JSON.parse(fields.slice(2).join("\t")).title === `Expo list ${tag}`);
  assert.equal(lists.length, 1, "discover the genuine preceding native-created list");
  const historicalItem = lists[0][0];
  assert.match(historicalItem, /^it_[a-z0-9]+$/);
  const messageForItem = (item) => page.getByTestId(/^message-row-/)
    .filter({ has: page.getByTestId(`miniapp-message-${item}`) });
  const historicalRow = messageForItem(historicalItem);
  await historicalRow.scrollIntoViewIfNeeded();
  const historicalRowId = await historicalRow.getAttribute("data-testid");
  assert.match(historicalRowId, /^message-row-.+$/);
  const historicalMessageId = historicalRowId.slice("message-row-".length);
  const historicalBody = await historicalRow.getByTestId(`message-body-${historicalMessageId}`).innerText();
  assert.ok(historicalBody.length > 0);
  await historicalRow.getByRole("button", { name: ui.messageActions(nativeAuthor), exact: true }).waitFor();
  assert.ok(historyBefore.split("\n").includes(`${nativeAuthor}: ${historicalBody}`),
    "the visible historical ID/body/author must match the genuine native peer");
  const counts = (history) => {
    const result = new Map();
    for (const line of history.split("\n").filter(Boolean)) {
      const bytes = Buffer.from(line).toString("base64");
      result.set(bytes, (result.get(bytes) ?? 0) + 1);
    }
    return result;
  };
  const beforeCounts = counts(historyBefore);
  await observeCanonicalReads(page);
  await page.goto(`${origin}/miniapps?space=${encodeURIComponent(space)}`, { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: ui.installDrawing, exact: true }).click();
  await page.getByRole("textbox", { name: ui.appTitle, exact: true }).fill(title);
  await page.getByRole("button", { name: ui.addApp, exact: true }).click();
  await page.waitForURL((url) => /^\/miniapp\/it_[a-z0-9]+$/.test(url.pathname));
  const id = new URL(page.url()).pathname.split("/").at(-1);
  assert.match(id, /^it_[a-z0-9]+$/);
  await page.goto(`${origin}/chat/${space}`, { waitUntil: "domcontentloaded" });
  const row = messageForItem(id);
  await row.scrollIntoViewIfNeeded();
  const rowId = await row.getAttribute("data-testid");
  assert.match(rowId, /^message-row-.+$/);
  const messageId = rowId.slice("message-row-".length);
  assert.notEqual(messageId, historicalMessageId, "new installation has its own original message ID");
  await row.getByRole("button", { name: ui.messageActions(browserAuthor), exact: true }).waitFor();
  const body = row.getByTestId(`message-body-${messageId}`);
  await body.waitFor();
  assert.equal(await body.innerText(), ui.appAdded);
  await cli(peerHome, "sync");
  const historyAfter = await cli(peerHome, "read", space);
  const afterCounts = counts(historyAfter);
  const newMessageBytes = Buffer.from(`${browserAuthor}: ${ui.appAdded}`).toString("base64");
  assert.equal((afterCounts.get(newMessageBytes) ?? 0) - (beforeCounts.get(newMessageBytes) ?? 0), 1,
    "native peer receives exactly one additional original App message from the browser author");
  for (const [bytes, count] of beforeCounts)
    assert.ok((afterCounts.get(bytes) ?? 0) >= count, "retain every genuine earlier native history line byte-for-byte");
  const preserved = page.getByTestId(`message-row-${historicalMessageId}`);
  await preserved.scrollIntoViewIfNeeded();
  assert.equal(await preserved.getByTestId(`message-body-${historicalMessageId}`).innerText(), historicalBody);
  await preserved.getByRole("button", { name: ui.messageActions(nativeAuthor), exact: true }).waitFor();
  // Reading older history can recycle the newest row. Reveal its genuine public
  // message link instead of assuming an offscreen FlashList row stays mounted.
  await page.goto(`${origin}/chat/${space}?jump=${encodeURIComponent(messageId)}`, { waitUntil: "domcontentloaded" });
  await row.scrollIntoViewIfNeeded();
  await screenshot("09a-current-app-install-localized-copy.png");
  await page.goto(`${origin}/miniapp/${id}`, { waitUntil: "domcontentloaded" });
  const selector = `iframe[title=${JSON.stringify(title)}]`;
  const frame = page.frameLocator(selector);
  const editor = frame.locator('main[data-canonical-editor="true"]');
  await editor.waitFor();
  assert.equal(await page.locator(selector).getAttribute("sandbox"), "allow-scripts");
  const initial = await readRenderedScene(frame);
  assert.deepEqual(initial.scene.elements, []);
  step("canonical-excalidraw-opens-in-strict-browser-sandbox", {
    installation: "actual-catalog-and-form-ui",
    itemId: id,
    newMessageId: messageId,
    newMessageAuthor: browserAuthor,
    visibleAppBody: ui.appAdded,
    nativePeerNewAppOccurrenceDelta: 1,
    historicalNativeAppMessage: { itemId: historicalItem, messageId: historicalMessageId, author: nativeAuthor, body: historicalBody },
    allHistoricalNativeLineBytesPreserved: true,
  });

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
