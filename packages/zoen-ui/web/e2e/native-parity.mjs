// Real WebKit layout/interaction checks against the explicit read-only native-core fixture.
// Does not claim relay, recording, file transport, or native pixel parity coverage.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const args = Object.fromEntries(process.argv.slice(2).map((value, i, all) => value.startsWith('--') ? [value.slice(2), all[i + 1]] : null).filter(Boolean));
if (!args.playwright || !args.url || !args.output) throw new Error('Use --playwright /path/to/playwright/index.mjs --url http://localhost:4174/?parity=1 --output /path');
const { webkit } = await import(pathToFileURL(resolve(args.playwright)).href);
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const fixture = JSON.parse(await readFile(new URL('../fixtures/native-parity-v1.json', import.meta.url), 'utf8'));
assert.ok(fixture.homeApps?.length > 0, 'actual native home snapshot export must be present');
await mkdir(args.output, { recursive: true });
const deadline = setTimeout(() => { console.error('FAIL WebKit UI check exceeded its 120-second deadline.'); process.exit(1); }, 120_000);
let browser;
console.log('WebKit matrix starting');
try {
  for (const viewport of ['narrow', 'wide']) for (const appearance of ['light', 'dark']) for (const screen of ['chats', 'chat', 'composer', 'spaces', 'settings', 'onboarding']) {
    if (screen === 'chats') { if (browser) await browser.close(); browser = await webkit.launch({ timeout: 20_000 }); }
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
    assert.equal(await page.locator('.zm-bubble .zm-meta').evaluateAll(elements => elements.every(element => { const meta = element.getBoundingClientRect(); const bubble = element.closest('.zm-bubble').getBoundingClientRect(); return meta.bottom <= bubble.bottom + 1 && meta.right <= bubble.right + 1; })), true, 'message timestamps stay inside their bubbles');
    if (screen === 'composer') {
      assert.deepEqual(await page.locator('.zm-send').first().evaluate(element => { const rect = element.getBoundingClientRect(); return [rect.width, rect.height]; }), [48, 48], 'SpaceView action is 48 logical points');
      assert.deepEqual(await page.locator('.zm-attach').first().evaluate(element => { const rect = element.getBoundingClientRect(); return [rect.width, rect.height]; }), [44, 44], 'ChatAttachmentButton is 44 logical points');
      assert.equal(await page.locator('textarea').inputValue(), 'See you tomorrow');
      await page.locator('textarea').fill('See you tomorrow again');
      assert.equal(await page.locator('textarea').inputValue(), 'See you tomorrow again');
      await page.locator('textarea').fill('See you tomorrow');
    }
    if (viewport === 'wide' && screen !== 'onboarding') {
      assert.equal(await page.locator('.na-sidebar').evaluate(element => element.getBoundingClientRect().width), 372);
      assert.equal(await page.locator('.na-rail').evaluate(element => element.getBoundingClientRect().width), 80);
      assert.equal(await page.locator('.na-chat-sidebar').evaluate(element => element.getBoundingClientRect().width), 280);
    }
    if (screen === 'chats' && viewport === 'narrow') {
      assert.deepEqual(await page.locator('.nw-card').evaluateAll(elements => elements.map(element => element.getAttribute('data-snapshot-id'))), fixture.homeApps.map(snapshot => snapshot.id), 'the canonical native home snapshots and order are preserved exactly');
      assert.equal(await page.locator('.nw-empty').count(), 0, 'a populated native fixture must not be replaced with an empty state');
      assert.equal(await page.locator('.zn-circle').first().evaluate(element => element.getBoundingClientRect().width), 56);
      assert.equal(await page.locator('.zn-tabs').evaluate(element => element.getBoundingClientRect().height), 62);
      assert.equal(await page.locator('.zm-chat').count(), 6);
      await page.getByRole('button', { name: 'Search', exact: true }).click();
      await page.getByRole('searchbox', { name: 'Search', exact: true }).fill('Marina');
      assert.equal(await page.locator('.zm-chat').count(), 2);
      await page.getByRole('button', { name: 'Cancel', exact: true }).click();
      assert.equal(await page.locator('.zm-chat').count(), 6);
    }
    if (screen === 'settings') {
      await page.getByRole('searchbox', { name: 'Search settings' }).fill('Theme');
      assert.equal(await page.locator('.ns-row').count(), 1);
      await page.getByRole('searchbox', { name: 'Search settings' }).fill('');
      await page.locator('.ns-row').filter({ hasText: 'Theme' }).click();
      await page.locator('.zw-account select[aria-label="Appearance"]').selectOption(appearance);
      assert.equal(await page.evaluate(() => document.documentElement.dataset.appearance), appearance);
      await page.getByRole('button', { name: 'Back to Settings' }).click();
    }
    if (screen === 'onboarding') assert.equal(await page.getByRole('button', { name: 'Hi, Zoen!', exact: true }).isEnabled(), true);
    assert.equal(await page.locator('img').evaluateAll(images => images.every(image => image.complete && image.naturalWidth > 0)), true, 'all actual native art loaded');
    const image = resolve(args.output, `web-${viewport}-${appearance}-${screen}.png`);
    await page.screenshot({ path: image });
    assert.deepEqual(errors, [], 'no uncaught browser errors');
    console.log(`PASS ${viewport}/${appearance}/${screen} ${image}`);
    await context.close();
  }
} finally { clearTimeout(deadline); await browser?.close(); }
