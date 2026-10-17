// Focused responsive-list regression using the production Messenger and native
// fixture, not a relay/MLS test. Run against a freshly bundled parity preview:
// node chat-navigation-regression.mjs <playwright/index.mjs> <url> <output-dir> [revision]
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { chatRow, openChat, showNarrowChats } from './chat-navigation.mjs';
const [playwright, url, output, sourceRevision] = process.argv.slice(2);
if (!playwright || !url || !output) throw new Error('Use chat-navigation-regression.mjs <playwright/index.mjs> <preview-url> <output-dir> [revision]');
const { chromium } = await import(pathToFileURL(resolve(playwright)).href);
const revision = sourceRevision ?? execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
await mkdir(output, { recursive: true });
const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME } : {});
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
  page.setDefaultTimeout(10_000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(url);
  await page.waitForFunction(() => !!window.__ZOEN_PARITY__);
  await page.evaluate(config => window.__ZOEN_PARITY__.render(config), { fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z', locale: 'en_US', timezone: 'UTC', seed: 128, source_revision: revision, appearance: 'light', screen: 'chat', viewport: 'wide' });
  const title = 'Paraty with Marina';
  // Same failure as two-people: the legacy copy exists, but is hidden on wide.
  const legacy = page.locator('.zm-chat-open', { hasText: title });
  assert.equal(await legacy.count(), 1);
  await assert.rejects(legacy.waitFor({ timeout: 500 }), /hidden|visible/);
  console.log('REPRODUCED: legacy .zm-chat-open waits on a hidden wide-list copy');
  await chatRow(page, title).waitFor();
  await chatRow(page, title).locator('.na-row-badge, .zm-badge').waitFor();
  assert.equal(await chatRow(page, title).count(), 1);
  await openChat(page, title);
  await page.screenshot({ path: resolve(output, 'wide-visible-chat.png') });
  await showNarrowChats(page);
  const navigation = page.getByRole('navigation', { name: 'Main navigation' });
  assert.equal(await navigation.count(), 1);
  assert.ok(await navigation.evaluate(element => element.getBoundingClientRect().top > innerHeight / 2), 'root navigation remains bottom-only');
  assert.equal(await page.locator('.zm-home-head').count(), 0, 'no retired top navigation');
  const row = chatRow(page, title);
  for (const action of ['Fixar', 'Silenciar']) {
    await row.hover();
    await row.locator('.zm-chat-more').click();
    await page.getByRole('menuitem', { name: action, exact: true }).click();
  }
  await row.locator('[aria-label="Fixada"]').waitFor();
  await row.locator('[aria-label="Silenciada"]').waitFor();
  await page.screenshot({ path: resolve(output, 'narrow-pinned-muted.png') });
  await page.setViewportSize({ width: 1280, height: 820 });
  await openChat(page, title);
  assert.equal(await page.locator('.zm-chat-head .zm-title-copy > b').textContent(), title);
  await showNarrowChats(page);
  await row.locator('[aria-label="Fixada"]').waitFor();
  await row.locator('[aria-label="Silenciada"]').waitFor();
  await row.hover();
  await row.locator('.zm-chat-more').click();
  await page.getByRole('menuitem', { name: 'Arquivar', exact: true }).click();
  await row.waitFor({ state: 'hidden' });
  await page.locator('.zm-archived-row').waitFor();
  await page.screenshot({ path: resolve(output, 'narrow-archived.png') });
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.locator('.na-shell').waitFor();
  assert.equal(await chatRow(page, title).count(), 0, 'archived chat is excluded from the visible wide list too');
  await openChat(page, 'Marina');
  assert.equal(await page.locator('.zm-chat-head .zm-title-copy > b').textContent(), 'Marina');
  assert.deepEqual(errors, []);
  console.log('PASS: visible exact-title selection, bottom-only narrow navigation, pin/mute persistence, archive and wide chat reopening');
} finally { await browser.close(); }
