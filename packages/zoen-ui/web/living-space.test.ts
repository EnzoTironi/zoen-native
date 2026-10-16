import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const output = resolve(import.meta.dirname, '../node_modules/.cache/living-space-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'living-space.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic' });
const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost/' });
Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: dom.window.navigator });
const { createElement: h, act, useState } = await import('react');
const { createRoot } = await import('react-dom/client');
const { LivingSpace, SpaceJoinNotifications, livingSpacesEnabled } = await import(pathToFileURL(output).href);
async function click(label: string) { const el = [...document.querySelectorAll('button')].find((b) => b.textContent === label); assert.ok(el); await act(async () => el.click()); }
test('living Space flag is default-off and storage refusal fails closed', () => {
  assert.equal(livingSpacesEnabled(undefined), false);
  assert.equal(livingSpacesEnabled({ getItem: () => null }), false);
  assert.equal(livingSpacesEnabled({ getItem: () => 'false' }), false);
  assert.equal(livingSpacesEnabled({ getItem: () => 'true' }), true);
  assert.equal(livingSpacesEnabled({ getItem: () => { throw Error('denied'); } }), false);
});
test('member home hides admin composer, opens existing page and conversations', async () => {
  const root = createRoot(document.getElementById('root')!); const opened: string[] = [];
  await act(async () => root.render(h(LivingSpace, { title: 'Garden', members: 4, announcements: [], pages: [{ id: 'page', title: 'Planting guide' }], canAnnounce: false, openPage: (id: string) => opened.push(id), conversations: h('p', null, 'Real conversation detail') })));
  try {
    assert.equal(document.querySelector('textarea'), null);
    assert.equal([...document.querySelectorAll('summary')].some((el) => el.textContent === 'Pin announcement'), false);
    await click('Planting guide'); assert.deepEqual(opened, ['page']);
    await click('General conversation›'); assert.match(document.body.textContent!, /Real conversation detail/);
  } finally { await act(async () => root.unmount()); }
});
test('root list, back navigation and page detail retain the selected Space', async () => {
  const root = createRoot(document.getElementById('root')!); const selected: string[] = [];
  function Harness() {
    const [detail, setDetail] = useState(false);
    return h(LivingSpace, { title: 'Garden', members: 4, announcements: [], pages: [{ id: 'page', title: 'Planting guide' }], canAnnounce: false,
      spaces: [{ id: 'garden', title: 'Garden' }], activeSpaceId: 'garden', selectSpace: (id: string) => selected.push(id),
      openPage: () => setDetail(true), conversations: h('p', null, 'Conversation detail'),
      detail: detail ? h('h1', null, 'Page detail') : undefined, detailTitle: 'Planting guide', closeDetail: () => { setDetail(false); return true; } });
  }
  await act(async () => root.render(h(Harness)));
  assert.equal(document.querySelector('.zl-sidebar-title')?.textContent, 'Zoen');
  assert.ok(document.querySelector('.zl-space-list button[aria-current="page"] svg'));
  const back = async (name: string) => { const button = document.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`); assert.ok(button); await act(async () => button.click()); };
  try {
    await back('Back to Spaces');
    assert.equal(document.querySelector('main h1')?.textContent, 'Spaces');
    const row = document.querySelector<HTMLButtonElement>('main button'); assert.ok(row); await act(async () => row.click());
    assert.deepEqual(selected, ['garden']); assert.equal(document.querySelector('main h1')?.textContent, 'Garden');
    await click('Planting guide'); assert.equal(document.querySelector('main h1')?.textContent, 'Page detail');
    await back('Back to Home'); assert.equal(document.querySelector('main h1')?.textContent, 'Garden');
    await click('General conversation›'); assert.match(document.querySelector('main')!.textContent!, /Conversation detail/);
    await back('Back to Home'); assert.equal(document.querySelector('main h1')?.textContent, 'Garden');
  } finally { await act(async () => root.unmount()); }
});
test('join approval invokes controller, failed approval is visible and request remains', async () => {
  const root = createRoot(document.getElementById('root')!); const calls: unknown[] = [];
  await act(async () => root.render(h(SpaceJoinNotifications, { requests: [{ id: 'sam', name: 'Sam', spaceTitle: 'Garden' }], decide: async (...args: unknown[]) => { calls.push(args); throw Error('Admin approval required'); } })));
  try { await click('Approve'); assert.deepEqual(calls, [['sam', true]]); assert.match(document.querySelector('[role=alert]')!.textContent!, /This request was not updated\. Check your access and try again\./); assert.match(document.body.textContent!, /Sam/); }
  finally { await act(async () => root.unmount()); }
});
