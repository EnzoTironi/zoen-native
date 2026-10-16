#!/usr/bin/env node
// Current-source mobile WebKit rendering. The application owns deterministic fixtures.
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';

const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, value, index, all) => {
  if (value.startsWith('--')) pairs.push([value.slice(2), all[index + 1]]);
  return pairs;
}, []));
if (!args.url || !args.output || !args.playwright) {
  throw new Error('Usage: node scripts/parity_web_capture.mjs --url <preview URL> --output <new directory> --playwright <path to playwright/index.mjs>');
}
const { webkit } = await import(pathToFileURL(resolve(args.playwright)).href);
const output = resolve(args.output);
await mkdir(output); // EEXIST is intentional: old frames must never survive a render failure.
const fixture = {
  fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z',
  locale: 'en_US', timezone: 'UTC', seed: 128,
  source_revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
};
const wide = args.viewport === 'wide';
const viewport = wide ? { width: 1376, height: 988 } : { width: 402, height: 812 };
const browser = await webkit.launch();
try {
  for (const screen of ['chats', 'chat', 'composer', 'spaces', 'notifications', 'profile', 'settings', 'onboarding']) {
    for (const appearance of ['light', 'dark']) {
      const context = await browser.newContext({ viewport,
        deviceScaleFactor: wide ? 2 : 3, isMobile: true, hasTouch: true, locale: 'en-US',
        timezoneId: 'UTC', colorScheme: appearance });
      try {
        const page = await context.newPage();
        await page.clock.setFixedTime(new Date(fixture.fixed_time));
        await page.goto(args.url, { waitUntil: 'networkidle' });
        await page.waitForFunction(() => typeof window.__ZOEN_PARITY__?.render === 'function');
        const applied = await page.evaluate(async ({ fixture, screen, appearance }) => {
          await window.__ZOEN_PARITY__.render({ ...fixture, screen, appearance, viewport: window.innerWidth > 700 ? 'wide' : 'narrow' });
          await document.fonts.ready;
          return window.__ZOEN_PARITY__.manifest;
        }, { fixture, screen, appearance });
        for (const [key, expected] of Object.entries(fixture)) {
          if (applied?.[key] !== expected) throw new Error(`Web fixture not applied: ${key}`);
        }
        // Renderer promises a settled deterministic phase. Do not disable CSS motion,
        // change colors, hide controls or inject a screenshot-only imitation.
        await page.screenshot({ path: resolve(output, `${screen}-${appearance}.png`), fullPage: false });
        await writeFile(resolve(output, 'manifest.json'), JSON.stringify(applied, null, 2) + '\n');
      } finally { await context.close(); }
    }
  }
} finally { await browser.close(); }
