// Real browser MediaRecorder + synthetic microphone device: UI/encoding lifecycle, not physical-mic or relay delivery proof.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const args = Object.fromEntries(process.argv.slice(2).map((value, i, all) => value.startsWith('--') ? [value.slice(2), all[i + 1]] : null).filter(Boolean));
if (!args.playwright || !args.url || !args.chrome) throw new Error('Use --playwright /path/index.mjs --url http://localhost:4174/?parity=1 --chrome /path/to/Chrome');
const { chromium } = await import(pathToFileURL(resolve(args.playwright)).href);
const browser = await chromium.launch({ executablePath: args.chrome, timeout: 20_000, args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'] });
try {
  const context = await browser.newContext({ viewport: { width: 402, height: 812 }, permissions: ['microphone'], locale: 'en-US', timezoneId: 'UTC' });
  const page = await context.newPage(); page.setDefaultTimeout(10_000);
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(args.url);
  await page.waitForFunction(() => !!window.__ZOEN_PARITY__);
  const config = { fixture_id:'native-parity-v1',fixed_time:'2026-01-15T12:00:00Z',locale:'en_US',timezone:'UTC',seed:128,source_revision:args.revision || execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),screen:'chats',appearance:'light',viewport:'narrow' };
  await page.evaluate(config => window.__ZOEN_PARITY__.render(config), config);
  // Native root action is a tap-to-compose sheet, not a voice-hold control.
  await page.getByRole('button', { name: 'New message', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'New chat', exact: true });
  await sheet.waitFor();
  assert.equal(await sheet.getByRole('tab', { name: 'Chat', exact: true }).getAttribute('aria-selected'), 'true');
  await sheet.getByRole('textbox', { name: 'Search by @handle' }).fill('marina');
  assert.equal(await sheet.locator('.native-avatar').count(), 1, 'genuine contact search filters the shared production sheet');
  await page.keyboard.press('Escape');
  await sheet.waitFor({ state: 'hidden' });
  console.log('PASS new-message tap opens genuine production contact sheet');
  await page.evaluate(config => window.__ZOEN_PARITY__.render(config), { ...config, screen: 'chat' });
  const recording = page.getByRole('group', { name: 'Gravando mensagem de voz' });
  async function hold() {
    const rect = await page.getByRole('button', { name: 'Gravar mensagem de voz', exact: true }).boundingBox(); assert.ok(rect);
    const point = { x: rect.x + rect.width/2, y: rect.y + rect.height/2 };
    await page.mouse.move(point.x, point.y); await page.mouse.down(); await recording.waitFor(); return point;
  }
  let point = await hold();
  await page.mouse.move(point.x - 130, point.y); await page.mouse.up();
  await recording.waitFor({ state: 'hidden' }); console.log('PASS real MediaRecorder hold + left cancel');
  point = await hold();
  await page.mouse.move(point.x, point.y - 100); await page.mouse.up();
  await page.getByRole('status').filter({ hasText: 'Recording locked' }).waitFor();
  assert.equal(await recording.isVisible(), true);
  await page.getByRole('button', { name: 'Descartar gravação' }).click(); await recording.waitFor({ state: 'hidden' });
  console.log('PASS real MediaRecorder hold + up lock + explicit cancel');
  await hold();
  await recording.locator('.zm-voice-time').filter({ hasText: '0:01' }).waitFor();
  await page.mouse.up(); await recording.waitFor({ state: 'hidden' });
  await page.getByRole('alert').filter({ hasText: 'Recarregue o Zoen' }).waitFor();
  console.log('PASS actual encoded clip reaches the production send callback; read-only fixture correctly refuses upload');
  assert.deepEqual(errors, []); await context.close();
} finally { await browser.close(); }
