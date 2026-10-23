// Real Expo controls + file chooser/download + canonical Rust peer. Synthetic data only.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function acceptMarkdownJourney({
  page,
  ui,
  origin,
  space,
  tag,
  privateRoot,
  peerHome,
  cli,
  screenshot,
  step,
}) {
  const title = `Expo Markdown ${tag}`;
  const original = `Anotação original ${tag} — café`;
  const edited = `Anotação revisada ${tag} — café ☕`;
  const task = `Revisar documento ${tag}`;
  const filename = `qa-document-${tag}.md`;
  const source = join(privateRoot, filename);
  await writeFile(source, `# ${title}\n\n${original}\n\n- [ ] ${task}\n`, "utf8");

  await page.goto(`${origin}/new-page?space=${encodeURIComponent(space)}`, {
    waitUntil: "domcontentloaded",
  });
  const choosing = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: ui.importMarkdown, exact: true }).click();
  await (await choosing).setFiles(source);
  await page.getByText(filename, { exact: true }).waitFor();
  await page.getByRole("button", { name: ui.importAndSharePage, exact: true }).click();
  await page.waitForURL((url) => /^\/page\/[^/]+$/.test(url.pathname), { timeout: 90_000 });
  const pageUrl = page.url();
  const itemId = decodeURIComponent(new URL(pageUrl).pathname.split("/").at(-1));
  assert.ok(itemId, "Markdown import must open its canonical item route");
  const block = (text) => page.getByRole("button", { name: ui.editBlock(text), exact: true });
  await block(original).waitFor({ timeout: 90_000 });
  assert.equal(
    await page.getByRole("checkbox", { name: task, exact: true }).getAttribute("aria-checked"),
    "false",
  );
  await page.getByText(ui.publishedPage, { exact: true }).waitFor({ timeout: 90_000 });
  step("markdown-file-chooser-imports-encrypted-page", { itemId });

  const taskCheckbox = page.getByRole("checkbox", { name: task, exact: true });
  await taskCheckbox.click();
  await page.getByRole("checkbox", { name: task, exact: true, checked: true }).waitFor();
  await page.getByText(ui.savedDraft, { exact: true }).waitFor();
  const checkedReload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(checkedReload?.status(), 200);
  await page.getByRole("checkbox", { name: task, exact: true, checked: true }).waitFor();
  await page.getByText(ui.savedDraft, { exact: true }).waitFor();
  await screenshot("markdown-checked-draft-after-cold-reload.png");
  await taskCheckbox.click();
  await page.getByRole("checkbox", { name: task, exact: true, checked: false }).waitFor();
  assert.equal(await taskCheckbox.getAttribute("aria-checked"), "false");
  // Returning to the published content clears the draft difference by design.
  await page.getByText(ui.publishedPage, { exact: true }).waitFor();
  assert.equal(
    await page.getByRole("button", { name: ui.sharePageChanges, exact: true }).isDisabled(),
    true,
  );
  step("markdown-checkbox-interaction-updates-visible-state-and-draft", {
    checkedDraftSurvivesColdReload: true,
    checkedThenUnchecked: true,
    returnsToPublishedState: true,
  });

  await block(original).click();
  await page.getByRole("textbox", { name: ui.blockContent, exact: true }).fill(edited);
  await page.getByRole("button", { name: ui.saveDraft, exact: true }).click();
  await page.waitForURL((url) => url.pathname === new URL(pageUrl).pathname);
  await block(edited).waitFor();
  await page.getByText(ui.savedDraft, { exact: true }).waitFor();
  const draftReload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(draftReload?.status(), 200);
  await block(edited).waitFor({ timeout: 90_000 });
  await page.getByText(ui.savedDraft, { exact: true }).waitFor();
  await screenshot("06-markdown-draft-after-cold-reload.png");
  step("markdown-block-edit-draft-survives-cold-route-reload");

  await page.getByRole("button", { name: ui.sharePageChanges, exact: true }).click();
  await page.getByText(ui.publishedPage, { exact: true }).waitFor({ timeout: 90_000 });
  await page.reload({ waitUntil: "domcontentloaded" });
  await block(edited).waitFor({ timeout: 90_000 });
  await page.getByText(ui.publishedPage, { exact: true }).waitFor({ timeout: 90_000 });
  assert.equal(
    await page.getByRole("alert").count(),
    0,
    "valid committed Markdown page must have no errors",
  );

  await page.getByRole("button", { name: ui.artifactDiscussion, exact: true }).click();
  await page.waitForURL((url) => url.pathname.startsWith(`/thread/${encodeURIComponent(space)}/`));
  const rootId = decodeURIComponent(new URL(page.url()).pathname.split("/").at(-1));
  assert.ok(rootId, "the artifact opens its original canonical post");
  await page.getByTestId(`message-row-${rootId}`).waitFor();
  const comment = `Artifact discussion ${tag}`;
  await page.locator('[data-testid="message-input"]:visible').fill(comment);
  await page.locator('[data-testid="send-message"]:visible').click();
  await page.locator('[data-testid^="message-body-"]:visible').filter({ hasText: comment }).waitFor();
  await page.goto(pageUrl, { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: ui.artifactDiscussionCount(1), exact: true }).click();
  await page.waitForURL((url) => url.pathname.startsWith(`/thread/${encodeURIComponent(space)}/`));
  assert.equal(decodeURIComponent(new URL(page.url()).pathname.split("/").at(-1)), rootId,
    "cold route reload keeps the same discussion root");
  await page.locator('[data-testid^="message-body-"]:visible').filter({ hasText: comment }).waitFor();
  await page.goto(pageUrl, { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: ui.exportMarkdown, exact: true }).waitFor();
  step("markdown-discussion-keeps-original-post-and-comment-after-cold-route-reload", { rootId });

  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: ui.exportMarkdown, exact: true }).click();
  const download = await downloading;
  assert.equal(download.suggestedFilename(), `${title}.md`);
  const browserPath = join(privateRoot, "browser-export.md");
  await download.saveAs(browserPath);
  const browserMarkdown = await readFile(browserPath, "utf8");
  assert.ok(browserMarkdown.includes(title));
  assert.ok(
    browserMarkdown.includes(edited),
    "browser download must contain the actual edited UTF-8 block",
  );
  assert.ok(browserMarkdown.includes(`[ ] ${task}`), "export must preserve the unchecked task");
  assert.equal(browserMarkdown.includes(original), false, "export must replace the old body");
  await screenshot("07-markdown-committed-export.png");
  step("markdown-commit-and-browser-download-preserve-edited-content");

  const nativePath = join(privateRoot, "native-export.md");
  await cli(peerHome, "page", "export", itemId, "--out", nativePath);
  const nativeMarkdown = await readFile(nativePath, "utf8");
  assert.equal(
    nativeMarkdown,
    browserMarkdown,
    "canonical peer must decrypt and export the exact same committed page",
  );
  step("markdown-browser-and-native-core-exports-match", {
    bytes: Buffer.byteLength(browserMarkdown),
    sha256: createHash("sha256").update(browserMarkdown).digest("hex"),
  });

  await page.goto(`${origin}/item/${encodeURIComponent(itemId)}?history=1`, {
    waitUntil: "domcontentloaded",
  });
  await page.getByRole("button").filter({ has: page.getByText("v1", { exact: true }) }).click();
  await page.getByText(original, { exact: true }).waitFor();
  const historicalDownload = page.waitForEvent("download");
  await page.getByTestId("page-version-export").click();
  const historical = await historicalDownload;
  assert.equal(historical.suggestedFilename(), `${title} v1.md`);
  const historicalPath = join(privateRoot, "browser-version-1.md");
  await historical.saveAs(historicalPath);
  const historicalBytes = await readFile(historicalPath, "utf8");
  assert.equal(historicalBytes, await readFile(source, "utf8"),
    "history must export the exact selected version, not the current document");
  assert.equal(historicalBytes.includes(edited), false);
  await screenshot("markdown-selected-version-export.png");
  step("markdown-history-download-preserves-selected-version", {
    version: 1,
    bytes: Buffer.byteLength(historicalBytes),
    sha256: createHash("sha256").update(historicalBytes).digest("hex"),
  });
}
