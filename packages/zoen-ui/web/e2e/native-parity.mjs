// Real WebKit layout/interaction checks against the explicit read-only native-core fixture.
// Does not claim relay, recording, file transport, or native pixel parity coverage.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const args = Object.fromEntries(process.argv.slice(2).map((value, i, all) => value.startsWith('--') ? [value.slice(2), all[i + 1]] : null).filter(Boolean));
if (!args.playwright || !args.url || !args.output) throw new Error('Use --playwright /path/to/playwright/index.mjs --url http://localhost:4174/?parity=1 --output /path');
const { webkit } = await import(pathToFileURL(resolve(args.playwright)).href);
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
await mkdir(args.output, { recursive: true });
const deadline = setTimeout(() => { console.error('FAIL WebKit UI check exceeded its 120-second deadline.'); process.exit(1); }, 120_000);
const browser = await webkit.launch({ timeout: 20_000 });
console.log('WebKit launched');
try {
  for (const viewport of ['narrow', 'wide']) for (const appearance of ['light', 'dark']) for (const screen of ['chats', 'chat', 'composer', 'spaces', 'settings', 'onboarding']) {
    const context = await browser.newContext({ viewport: viewport === 'narrow' ? { width: 402, height: 778 } : { width: 1376, height: 988 }, deviceScaleFactor: viewport === 'narrow' ? 3 : 2, locale: 'en-US', timezoneId: 'UTC', colorScheme: appearance });
    const page = await context.newPage();
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.clock.setFixedTime(new Date('2026-01-15T12:00:00Z'));
    page.setDefaultTimeout(10_000);
    await page.goto(args.url, { timeout: 10_000 });
    await page.waitForFunction(() => !!window.__ZOEN_PARITY__);
    await page.evaluate(config => window.__ZOEN_PARITY__.render(config), { fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z', locale: 'en_US', timezone: 'UTC', seed: 128, source_revision: revision, appearance, screen, viewport });
    assert.equal(await page.locator('.native-parity').getAttribute('data-screen'), screen);
    if (screen !== 'onboarding') assert.equal(await page.locator('.zm-native-production').count(), 1, 'actual production Messenger is rendered');
    if (viewport === 'wide' && screen !== 'onboarding') assert.equal(await page.locator('.na-shell').count(), 1, 'production adaptive shell is rendered');
    assert.equal(await page.evaluate(() => window.__ZOEN_PARITY__.manifest.source_revision), revision);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'no horizontal overflow');
    if (screen === 'composer') {
      assert.equal(await page.locator('textarea').inputValue(), 'See you tomorrow');
      await page.locator('textarea').fill('See you tomorrow again');
      assert.equal(await page.locator('textarea').inputValue(), 'See you tomorrow again');
      await page.locator('textarea').fill('See you tomorrow');
    }
    if (screen === 'chats' && viewport === 'narrow') {
      assert.equal(await page.locator('.zm-chat').count(), 6);
      await page.getByRole('button', { name: 'Search', exact: true }).click();
      await page.getByRole('searchbox', { name: 'Search', exact: true }).fill('Marina');
      assert.equal(await page.locator('.zm-chat').count(), 2);
      await page.getByRole('button', { name: 'Cancel', exact: true }).click();
      assert.equal(await page.locator('.zm-chat').count(), 6);
    }
    const image = resolve(args.output, `web-${viewport}-${appearance}-${screen}.png`);
    await page.screenshot({ path: image });
    assert.deepEqual(errors, [], 'no uncaught browser errors');
    console.log(`PASS ${viewport}/${appearance}/${screen} ${image}`);
    await context.close();
  }
} finally { clearTimeout(deadline); await browser.close(); }
