import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function acceptMediaJourney({
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
  const name = `expo-attachment-${tag}.txt`;
  const bytes = Buffer.from(`Synthetic Expo attachment ${tag}\nNo personal data.\n`, "utf8");
  const path = join(privateRoot, name);
  await writeFile(path, bytes);
  await page.goto(`${origin}/chat/${space}`, { waitUntil: "domcontentloaded" });
  await page.getByTestId("add-attachment").click();
  const choosing = page.waitForEvent("filechooser");
  // Click with a mouse: keyboard activation would conceal a pointer-events fence
  // accidentally inherited by the universal Host's browser dialog.
  await page.getByRole("button", { name: ui.chooseFile, exact: true }).click();
  await (await choosing).setFiles(path);
  await page.getByTestId("attachment-draft-strip").waitFor();
  await page.getByTestId("send-attachments").click();
  const openFile = page.getByRole("button", { name: ui.openFile(name), exact: true });
  await openFile.waitFor();
  step("mouse-attachment-menu-and-file-chooser-publish-encrypted-file");

  await openFile.click();
  await page.getByText(bytes.toString("utf8").trim(), { exact: true }).waitFor();
  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: ui.downloadFile, exact: true }).click();
  const download = await downloading;
  const downloaded = join(privateRoot, `browser-${name}`);
  await download.saveAs(downloaded);
  assert.deepEqual(await readFile(downloaded), bytes);
  await page.getByRole("button", { name: ui.closePreview, exact: true }).click();
  await screenshot("09-encrypted-file-browser-roundtrip.png");

  const files = await cli(peerHome, "files", space);
  const row = files.split("\n").find((line) => line.includes(`\t${name}\t`));
  assert.ok(row, "canonical native peer must receive the browser-uploaded file");
  const id = row.split("\t")[0];
  assert.match(id, /^it_[a-z0-9]+$/);
  const native = join(privateRoot, `native-${name}`);
  await cli(peerHome, "file", "get", id, "--out", native);
  assert.deepEqual(await readFile(native), bytes);
  step("browser-and-canonical-native-peer-download-exact-attachment-bytes", {
    bytes: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}
