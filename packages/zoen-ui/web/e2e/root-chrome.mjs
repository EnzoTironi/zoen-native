import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const [playwright, url, output] = process.argv.slice(2);
if (!playwright || !url || !output) throw new Error('Use root-chrome.mjs <playwright/index.mjs> <preview-url> <output>');
const { webkit } = await import(pathToFileURL(resolve(playwright)).href);
await mkdir(output, { recursive: true });
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const browser = await webkit.launch();
try {
  for (const appearance of ['light', 'dark']) for (const viewport of ['narrow', 'wide']) {
    const context = await browser.newContext({ viewport: viewport === 'narrow' ? { width: 402, height: 812 } : { width: 1376, height: 988 }, colorScheme: appearance });
    const page = await context.newPage();
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.goto(url); await page.waitForFunction(() => !!window.__ZOEN_PARITY__);
    for (const screen of ['chats', 'spaces', 'notifications', 'profile']) {
      await page.evaluate(config => window.__ZOEN_PARITY__.render(config), { fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z', locale: 'en_US', timezone: 'UTC', seed: 128, source_revision: revision, appearance, screen, viewport });
      assert.equal(await page.locator('.zm-home-head, .native-root-screen > h1').count(), 0);
      assert.equal(await page.locator('[aria-label="Offline"]:not(.zn-navigation [aria-label="Offline"])').count(), 0);
      assert.equal(await page.getByRole('navigation', { name: 'Main navigation' }).count(), 1);
      await page.screenshot({ path: resolve(output, `${viewport}-${appearance}-${screen}.png`) });
      console.log(`PASS ${viewport}/${appearance}/${screen}`);
    }
    assert.deepEqual(errors, []);
    await context.close();
  }
} finally { await browser.close(); }
