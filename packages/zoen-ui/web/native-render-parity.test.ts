import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { nativeFixture } from './native-fixture-store.ts';

const revision = 'a'.repeat(40);
const output = resolve(import.meta.dirname, '../node_modules/.cache/native-render-parity-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-parity.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' }, define: { ZOEN_SOURCE_REVISION: JSON.stringify(revision) } });
const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost/' });
const media = (query: string) => ({ matches: query.includes('min-width') ? false : false, addEventListener() {}, removeEventListener() {} });
Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, localStorage: dom.window.localStorage, matchMedia: media, requestAnimationFrame: (callback: (time: number) => void) => setTimeout(() => callback(0), 0) });
Object.defineProperty(dom.window, 'matchMedia', { value: media });
Object.defineProperty(dom.window, 'innerWidth', { value: 402 });
Object.defineProperty(globalThis, 'innerWidth', { value: 402 });
Object.defineProperty(globalThis, 'innerHeight', { value: 812 });
Object.defineProperty(globalThis, 'devicePixelRatio', { value: 3 });
Object.defineProperty(document, 'fonts', { value: { ready: Promise.resolve() } });
document.getAnimations = () => [];
dom.window.HTMLImageElement.prototype.decode = async () => {};
dom.window.HTMLElement.prototype.scrollIntoView = () => {};
const { installNativeParity } = await import(pathToFileURL(output).href);
installNativeParity(document.getElementById('root')!);
async function render(screen: string) {
  await dom.window.__ZOEN_PARITY__!.render({ fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z', locale: 'en_US', timezone: 'UTC', seed: 128, source_revision: revision, screen, appearance: 'light', viewport: 'narrow' });
}

test('Spaces parity follows the native community-opening journey, not the removed root tab', async () => {
  await render('spaces');
  const coastal = nativeFixture.spaces.find(space => space.kind === 'community')!;
  assert.equal(document.querySelector('.zm-app')?.getAttribute('data-selected-space'), coastal.id);
  assert.equal(document.querySelector('[data-testid="spaces-directory"]'), null);
  assert.match(document.querySelector('.zm-title-copy')!.textContent!, /Coastal Travelers/);
});

test('compact notifications use the same dismissible sheet reached from the Home card', async () => {
  await render('chats');
  const button = document.querySelector<HTMLButtonElement>('.nw-notifications')!;
  button.click();
  await new Promise(resolve => setTimeout(resolve, 0));
  const sheet = document.querySelector('[role="dialog"][aria-label="Notifications"]');
  assert.ok(sheet, 'Home notifications opens a sheet, not a fourth root destination');
  assert.ok(sheet.querySelector('button[aria-label="Close notifications"]'));
  assert.equal(sheet.querySelector('.zn-navigation'), null);
});

test('notifications card preserves native bell/count/title/detail hierarchy', async () => {
  await render('chats');
  const card = document.querySelector('.nw-notifications')!;
  assert.ok(card.querySelector('svg'), 'native NotificationInboxCard starts with bell and open glyphs');
  const labels = [...card.querySelectorAll('.nw-value,.nw-title,.nw-detail')].map(element => element.textContent);
  assert.deepEqual(labels, ['3', 'Notifications', 'Pending requests']);
});

test('settings stays over the existing Profile and preserves native account capability', async () => {
  await render('settings');
  assert.ok(document.querySelector('.ns-overlay > .native-settings[role=dialog]'));
  assert.ok(document.querySelector('.zm-main .nr-profile'));
  assert.equal(document.querySelector('.ns-profile'), null, 'no enrolled sync account means no profile card in Settings');
  document.querySelector<HTMLButtonElement>('button[aria-label="Close settings"]')!.click();
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(document.querySelector('.ns-overlay'), null);
  assert.ok(document.querySelector('.nr-profile'));
});

test('native chat renders genuine reactions and inline time reservations', async () => {
  await render('chat');
  const marina = nativeFixture.spaces.find(space => space.counterpart?.handle === 'marina')!;
  assert.equal(document.querySelectorAll('.zm-reactions button').length, marina.entries.reduce((count, entry) => count + entry.reactions.length, 0));
  assert.ok(document.querySelector('.zm-meta-reservation'));
});

test('community messages use actual agent identities and projected hike app state', async () => {
  await render('spaces');
  assert.ok(document.querySelector('.native-hike-card'), 'native hike renderer consumes actual item app viewJson, not a generic text item card');
  const guide = nativeFixture.personas.find(persona => persona.handle === 'guia')!;
  assert.ok(document.querySelector(`.zm-sender-face [aria-label="${guide.name}"] img`), 'agent author is not replaced by a gray person monogram');
});

test('native inbox keeps unread totals separate from mentions and uses native face size', async () => {
  await render('chats');
  const row = document.querySelector('.zm-chat-open')!;
  assert.equal(row.querySelector<HTMLElement>('.native-avatar')!.style.width, '56px');
  assert.equal(row.querySelector('.zm-badge:not(.zm-mention)')!.textContent, String(nativeFixture.spaces[0]!.unread));
});
