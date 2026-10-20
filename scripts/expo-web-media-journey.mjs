import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

// These are public rendered rows and labels, not browser/core state inspection.
export async function acceptMediaJourney({
  page,
  ui,
  browserAuthor,
  origin,
  space,
  tag,
  privateRoot,
  peerHome,
  cli,
  until,
  screenshot,
  step,
}) {
  assert.ok(browserAuthor, "the real browser author is required for scoped message actions");
  assert.equal(typeof until, "function", "use the runner's bounded public-peer wait");
  const chatPath = `/chat/${encodeURIComponent(space)}`;
  const cases = [
    {
      kind: "main",
      name: `expo-attachment-${tag}.txt`,
      bytes: Buffer.from(`Synthetic Expo attachment ${tag}\nNo personal data.\n`, "utf8"),
      caption: `  Exact attachment ${tag} 🗂️\nKeep this line and spaces.  `,
    },
    {
      kind: "inline",
      name: `expo-attachment-inline-${tag}.txt`,
      bytes: Buffer.from(`Synthetic inline attachment ${tag}\nExact reply bytes.\n`, "utf8"),
      caption: `  Inline attachment ${tag} 🧵\nKeep this reply and spaces.  `,
    },
    {
      kind: "thread",
      name: `expo-attachment-thread-${tag}.txt`,
      bytes: Buffer.from(`Synthetic thread attachment ${tag}\nExact thread bytes.\n`, "utf8"),
      caption: `  Thread attachment ${tag} 📎\nKeep this thread and spaces.  `,
    },
  ];
  for (const entry of cases) {
    entry.path = join(privateRoot, entry.name);
    await writeFile(entry.path, entry.bytes);
  }
  const main = cases[0];
  const inline = cases[1];
  const thread = cases[2];
  const occurrences = (text, needle) => text.split(needle).length - 1;
  const fileLines = (output) => output.split("\n").filter(Boolean);
  const matchingFileRows = (output, name) =>
    fileLines(output).filter((line) => line.includes(`\t${name}\t`));
  const filesBefore = await cli(peerHome, "files", space);
  const oldItemIds = new Set(fileLines(filesBefore).map((line) => line.split("\t")[0]));
  for (const entry of cases)
    assert.equal(matchingFileRows(filesBefore, entry.name).length, 0, "use fresh synthetic filenames");

  async function fileRow(entry) {
    const open = page.getByRole("button", { name: ui.openFile(entry.name), exact: true });
    await open.first().waitFor({ timeout: 60_000 });
    assert.equal(await open.count(), 1, "one visible original attachment control for this filename");
    const row = page.locator('[data-testid^="message-row-"]:visible').filter({ has: open });
    assert.equal(await row.count(), 1, "the control belongs to one actual visible message row");
    await row.scrollIntoViewIfNeeded();
    const testId = await row.getAttribute("data-testid");
    assert.match(testId, /^message-row-.+$/);
    const messageId = testId.slice("message-row-".length);
    if (entry.messageId)
      assert.equal(messageId, entry.messageId, "reload retains the original rendered message ID");
    else entry.messageId = messageId;
    const caption = row.getByTestId(`message-caption-${messageId}`);
    await caption.waitFor();
    assert.equal(await caption.textContent(), entry.caption, "rendered caption preserves exact UTF-8 text");
    return row;
  }

  async function chooseAndSend(entry, rootCaption) {
    // Native-stack routes retain the previous, hidden composer in the DOM.
    const trigger = page.locator('[data-testid="add-attachment"]:visible');
    await trigger.waitFor();
    assert.equal(await trigger.count(), 1, "one visible attachment trigger on the current route");
    await trigger.click();
    const choosing = page.waitForEvent("filechooser");
    // Keep the actual mouse click: keyboard activation could conceal a pointer-events fence.
    await page.getByRole("menuitem", { name: ui.chooseFile, exact: true }).click();
    await (await choosing).setFiles(entry.path);
    await page.getByTestId("attachment-draft-strip").waitFor();
    const caption = page.getByRole("textbox", { name: ui.attachmentCaption(1, 1), exact: true });
    await caption.waitFor();
    assert.equal(await caption.getAttribute("data-testid"), "message-input");
    assert.equal(await caption.isEditable(), true, "the actual Web caption field must be editable");
    if (rootCaption) {
      const context = page.getByTestId("reply-message-context");
      await context.waitFor();
      assert.ok((await context.textContent()).includes(rootCaption), "picked file retains the chosen root context");
    }
    await caption.fill(entry.caption);
    assert.equal(await caption.inputValue(), entry.caption, "caption is entered through the public field");
    await page.getByTestId("send-attachments").click();
    const row = await fileRow(entry);
    await row.getByTestId(/^message-receipt-(sent|delivered|read)$/).waitFor({ timeout: 60_000 });
    await page.getByTestId("attachment-draft-strip").waitFor({ state: "hidden" });
    return row;
  }

  async function rootAction(label) {
    const root = await fileRow(main);
    await root.getByRole("button", { name: ui.messageActions(browserAuthor), exact: true }).click();
    await page.getByRole("button", { name: label, exact: true }).click();
  }

  await page.goto(`${origin}${chatPath}`, { waitUntil: "domcontentloaded" });
  await chooseAndSend(main);
  step("mouse-attachment-menu-and-file-chooser-publish-exact-caption", {
    messageId: main.messageId,
    exactCaption: main.caption,
    captionEntry: "actual editable public field",
  });

  // Preserve the original browser preview/download and exact-byte journey.
  await page.getByRole("button", { name: ui.openFile(main.name), exact: true }).click();
  await page.getByText(main.bytes.toString("utf8").trim(), { exact: true }).waitFor();
  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: ui.downloadFile, exact: true }).click();
  const download = await downloading;
  const downloaded = join(privateRoot, `browser-${main.name}`);
  await download.saveAs(downloaded);
  assert.deepEqual(await readFile(downloaded), main.bytes);
  await page.getByRole("button", { name: ui.closePreview, exact: true }).click();
  await screenshot("09-encrypted-file-browser-roundtrip.png");

  await rootAction(ui.reply);
  const context = page.getByTestId("reply-message-context");
  await context.waitFor();
  assert.ok((await context.textContent()).includes(main.caption));
  const inlineRow = await chooseAndSend(inline, main.caption);
  const quote = inlineRow.getByRole("button", { name: ui.goToQuotedMessage, exact: true });
  await quote.waitFor();
  assert.ok((await quote.textContent()).includes(main.caption), "inline quote renders the selected root");
  await quote.click();
  await fileRow(main);
  await screenshot("09b-attachment-inline-reply-caption.png");
  step("attachment-inline-reply-uses-public-root-action-and-exact-caption", {
    rootMessageId: main.messageId,
    messageId: inline.messageId,
    exactCaption: inline.caption,
    canonicalPeerTarget: "NOT_PROVEN",
  });

  await rootAction(ui.openReplies);
  const threadPath = `/thread/${encodeURIComponent(space)}/${encodeURIComponent(main.messageId)}`;
  await page.waitForURL((url) => url.pathname === threadPath);
  await chooseAndSend(thread, main.caption);
  await screenshot("09c-attachment-thread-caption.png");
  step("attachment-thread-entered-through-public-root-action", {
    rootMessageId: main.messageId,
    messageId: thread.messageId,
    exactCaption: thread.caption,
    canonicalPeerTarget: "NOT_PROVEN",
  });

  let peerFiles;
  await until(async () => {
    await cli(peerHome, "sync");
    peerFiles = await cli(peerHome, "files", space);
    return cases.every((entry) => matchingFileRows(peerFiles, entry.name).length > 0);
  }, "public peer did not receive all three actual attachment items", 90_000);
  for (const entry of cases) {
    const matches = matchingFileRows(peerFiles, entry.name);
    assert.equal(matches.length, 1, "public peer has exactly one item for each synthetic filename");
    entry.itemId = matches[0].split("\t")[0];
    assert.match(entry.itemId, /^it_[a-z0-9]+$/);
    assert.equal(oldItemIds.has(entry.itemId), false, "attachment is a new canonical peer item");
    const native = join(privateRoot, `native-${entry.name}`);
    await cli(peerHome, "file", "get", entry.itemId, "--out", native);
    assert.deepEqual(await readFile(native), entry.bytes, "public peer decrypts the exact selected bytes");
  }
  assert.equal(new Set(cases.map((entry) => entry.itemId)).size, cases.length);
  let peerHistory = await cli(peerHome, "read", space);
  for (const entry of cases)
    assert.equal(occurrences(peerHistory, `${browserAuthor}: ${entry.caption}`), 1,
      "public raw timeline output preserves each exact caption once");
  step("browser-and-canonical-native-peer-download-exact-attachment-bytes", {
    files: cases.map((entry) => ({
      kind: entry.kind, itemId: entry.itemId, bytes: entry.bytes.length,
      sha256: createHash("sha256").update(entry.bytes).digest("hex"),
    })),
    peerAllThreeExactCaptionOccurrences: 1,
    peerReadIncludesThreadEntries: true,
    peerThreadCaptionAndTargets: "NOT_PROVEN",
  });

  const threadReload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(threadReload?.status(), 200);
  assert.equal(new URL(page.url()).pathname, threadPath);
  await fileRow(main);
  await fileRow(thread);
  await page.goto(`${origin}${chatPath}`, { waitUntil: "domcontentloaded" });
  const root = await fileRow(main);
  await fileRow(inline);
  assert.equal(await page.getByRole("button", { name: ui.openFile(thread.name), exact: true }).count(), 0);
  await root.getByRole("button", { name: ui.viewReplies(1), exact: true }).waitFor();
  const mainReload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(mainReload?.status(), 200);
  await fileRow(main);
  const inlineRestored = await fileRow(inline);
  const restoredQuote = inlineRestored.getByRole("button", { name: ui.goToQuotedMessage, exact: true });
  assert.ok((await restoredQuote.textContent()).includes(main.caption));
  await (await fileRow(main)).getByRole("button", { name: ui.viewReplies(1), exact: true }).click();
  await page.waitForURL((url) => url.pathname === threadPath);
  await fileRow(thread);
  await screenshot("09d-attachment-caption-target-reload.png");

  await cli(peerHome, "sync");
  const afterReload = await cli(peerHome, "files", space);
  const retainedItemIds = new Set(fileLines(afterReload).map((line) => line.split("\t")[0]));
  for (const id of oldItemIds) assert.ok(retainedItemIds.has(id), "retain earlier public peer items");
  for (const entry of cases) {
    const matches = matchingFileRows(afterReload, entry.name);
    assert.equal(matches.length, 1, "reload adds no duplicate canonical peer attachment item");
    assert.equal(matches[0].split("\t")[0], entry.itemId, "reload retains the original canonical item ID");
  }
  peerHistory = await cli(peerHome, "read", space);
  for (const entry of cases)
    assert.equal(occurrences(peerHistory, `${browserAuthor}: ${entry.caption}`), 1,
      "reload adds no duplicate exact-caption raw history line");
  const qualification = {
    scope: "BOUNDED_PUBLIC_UI_AND_PEER_FILE_BYTES_ITEM_IDS",
    cases: cases.map((entry) => ({ kind: entry.kind, messageId: entry.messageId, itemId: entry.itemId })),
    exactRenderedCaptionsAfterReload: true,
    originalRenderedMessageIdsRetained: true,
    oneRenderedThreadReplyUnderChosenRoot: true,
    exactPeerBytesAndOriginalItemIds: true,
    exactPeerCaptionsReceivedOnce: true,
    canonicalPeerThreadPlacement: "NOT_PROVEN",
    canonicalPeerReplyAndThreadTargets: "NOT_PROVEN",
    canonicalPeerMessageIdUniqueness: "NOT_PROVEN",
    reason: "Current public read prints raw timeline author/body including thread entries; files exposes item IDs and file get exposes bytes. Neither exposes canonical reply/thread/message metadata.",
  };
  step("attachment-ui-reload-and-public-peer-item-identity-bounded-check", qualification);
  // Leave the main route ready for the next existing journey.
  await page.goto(`${origin}${chatPath}`, { waitUntil: "domcontentloaded" });
  await fileRow(main);
  return qualification;
}
