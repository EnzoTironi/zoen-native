// Real Expo controls + file chooser/download + canonical Rust peer. Synthetic data only.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function acceptMarkdownJourney({
  page,
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
  await page.getByRole("button", { name: "Importar arquivo Markdown", exact: true }).click();
  await (await choosing).setFiles(source);
  await page.getByText(filename, { exact: true }).waitFor();
  await page.getByRole("button", { name: "Importar e compartilhar página", exact: true }).click();
  await page.waitForURL((url) => /^\/page\/[^/]+$/.test(url.pathname), { timeout: 90_000 });
  const pageUrl = page.url();
  const itemId = decodeURIComponent(new URL(pageUrl).pathname.split("/").at(-1));
  assert.ok(itemId, "Markdown import must open its canonical item route");
  const block = (text) => page.getByRole("button", { name: `Editar bloco: ${text}`, exact: true });
  await block(original).waitFor({ timeout: 90_000 });
  assert.equal(
    await page.getByRole("checkbox", { name: task, exact: true }).getAttribute("aria-checked"),
    "false",
  );
  await page.getByText("Compartilhada na conversa", { exact: true }).waitFor({ timeout: 90_000 });
  step("markdown-file-chooser-imports-encrypted-page", { itemId });

  const taskCheckbox = page.getByRole("checkbox", { name: task, exact: true });
  await taskCheckbox.click();
  await page.getByRole("checkbox", { name: task, exact: true, checked: true }).waitFor();
  await page.getByText("Rascunho salvo neste aparelho", { exact: true }).waitFor();
  await taskCheckbox.click();
  await page.getByRole("checkbox", { name: task, exact: true, checked: false }).waitFor();
  assert.equal(await taskCheckbox.getAttribute("aria-checked"), "false");
  await page.getByText("Rascunho salvo neste aparelho", { exact: true }).waitFor();
  step("markdown-checkbox-interaction-updates-visible-state-and-draft", {
    checkedThenUnchecked: true,
  });

  await block(original).click();
  await page
    .getByRole("textbox", { name: "Conteúdo do bloco em Markdown", exact: true })
    .fill(edited);
  await page.getByRole("button", { name: "Guardar rascunho", exact: true }).click();
  await page.waitForURL((url) => url.pathname === new URL(pageUrl).pathname);
  await block(edited).waitFor();
  await page.getByText("Rascunho salvo neste aparelho", { exact: true }).waitFor();
  const draftReload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(draftReload?.status(), 200);
  await block(edited).waitFor({ timeout: 90_000 });
  await page.getByText("Rascunho salvo neste aparelho", { exact: true }).waitFor();
  await screenshot("06-markdown-draft-after-cold-reload.png");
  step("markdown-block-edit-draft-survives-cold-route-reload");

  await page.getByRole("button", { name: "Compartilhar alterações", exact: true }).click();
  await page.getByText("Compartilhada na conversa", { exact: true }).waitFor({ timeout: 90_000 });
  await page.reload({ waitUntil: "domcontentloaded" });
  await block(edited).waitFor({ timeout: 90_000 });
  await page.getByText("Compartilhada na conversa", { exact: true }).waitFor({ timeout: 90_000 });
  assert.equal(
    await page.getByRole("alert").count(),
    0,
    "valid committed Markdown page must have no errors",
  );

  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: "Exportar Markdown", exact: true }).click();
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
}
