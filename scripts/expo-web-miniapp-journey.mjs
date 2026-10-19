import assert from "node:assert/strict";

// The production host intentionally omits allow-forms. Only a real sandboxed
// browser catches an Add button that depends on an HTML submit event.
export async function acceptBuiltinMiniappJourney({
  page,
  ui,
  origin,
  space,
  tag,
  peerHome,
  cli,
  screenshot,
  step,
}) {
  const title = `Expo list ${tag}`;
  const first = `Bring water ${tag}`;
  const added = `Bring fruit ${tag}`;
  const keyboard = `Bring bread ${tag}`;
  const installed = await cli(
    peerHome,
    "app",
    space,
    "install",
    "list",
    JSON.stringify({ title, items: [first] }),
  );
  const id = installed.trim();
  assert.match(id, /^it_[a-z0-9]+$/);
  await page.goto(`${origin}/miniapp/${id}`, { waitUntil: "domcontentloaded" });
  const frame = page.frameLocator(`iframe[title=${JSON.stringify(title)}]`);
  const item = frame.getByRole("checkbox", { name: first, exact: true });
  await item.waitFor();
  assert.equal(
    await page.locator(`iframe[title=${JSON.stringify(title)}]`).getAttribute("sandbox"),
    "allow-scripts",
    "form interactions must work without relaxing the guest sandbox",
  );
  await item.click();
  // This controlled checkbox waits for the canonical MCP response; its checked
  // state must be observed after that response, not immediately after a click.
  await frame.getByRole("checkbox", { name: new RegExp(`^${first}`), checked: true }).waitFor();
  const input = frame.getByRole("textbox", { name: ui.builtinListInput, exact: true });
  await input.fill(added);
  await frame.getByRole("button", { name: ui.builtinAdd, exact: true }).click();
  await frame.getByRole("checkbox", { name: added, exact: true }).waitFor();
  await input.fill(keyboard);
  await input.press("Enter");
  await frame.getByRole("checkbox", { name: keyboard, exact: true }).waitFor();
  step("sandboxed-builtin-list-toggle-button-and-keyboard-use-canonical-mcp-tools");

  const reload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(reload?.status(), 200);
  await frame.getByRole("checkbox", { name: added, exact: true }).waitFor();
  await frame.getByRole("checkbox", { name: keyboard, exact: true }).waitFor();
  assert.equal(await frame.getByRole("checkbox", { name: new RegExp(first) }).isChecked(), true);
  await screenshot("08-builtin-list-after-cold-reload.png");
  step("builtin-list-cold-route-reload-retains-canonical-changes");

  const peer = await cli(peerHome, "app", space, "view");
  const row = peer.split("\n").find((line) => line.startsWith(`${id}\tlist\t`));
  assert.ok(row, "canonical native peer must receive the browser-updated list");
  const state = JSON.parse(row.slice(`${id}\tlist\t`.length));
  assert.deepEqual(
    state.items.map((entry) => entry.text),
    [first, added, keyboard],
  );
  assert.equal(state.items[0].done, true);
  step("builtin-list-browser-changes-match-canonical-native-peer");
}
