// Only target the current adaptive list, never the hidden detail-side copy.
// Exact titles avoid matching a preview that happens to mention another person.
export const chatRow = (page, title) => page.locator('.na-chat-row:visible, .zm-chat:visible')
  .filter({ has: page.locator('b').getByText(title, { exact: true }) });

export async function openChat(page, title) {
  if (page.viewportSize().width >= 768) await page.locator('.na-shell').waitFor();
  const row = chatRow(page, title);
  await row.waitFor();
  if (await row.locator('.zm-chat-open').count()) await row.locator('.zm-chat-open').click();
  else await row.click();
}

// List preferences live in the phone list. Do not resurrect the retired desktop
// list or top navigation merely to make the E2E selectors pass.
export async function showNarrowChats(page) {
  const closeThread = page.getByRole('button', { name: 'Fechar thread', exact: true });
  if (await closeThread.isVisible()) await closeThread.click();
  await page.setViewportSize({ width: 402, height: 812 });
  await page.locator('.na-shell').waitFor({ state: 'hidden' });
  const back = page.getByRole('button', { name: 'Back to Chats', exact: true });
  if (await back.isVisible()) await back.click();
  await page.locator('.zm-sidebar:visible').waitFor();
  await page.getByRole('navigation', { name: 'Main navigation' }).waitFor();
}
