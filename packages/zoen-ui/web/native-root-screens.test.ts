import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const output = resolve(import.meta.dirname, '../node_modules/.cache/native-root-screens-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-root-screens.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' } });
const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost/' });
Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: dom.window.navigator });
const { createElement: h, act } = await import('react');
const { createRoot } = await import('react-dom/client');
const { NativeNotifications, NativeProfile, NativeSpacesDirectory, nativeOpenTasks, nativeConnectionLabel } = await import(pathToFileURL(output).href);
const root = createRoot(document.getElementById('root')!);
const text = () => document.getElementById('root')!.textContent!;
const person = { id: '12345678901234567890', kind: 'person', name: 'Owner', handle: 'owner' };
async function render(component: unknown, props: object) { await act(async () => root.render(h(component as never, props))); }
async function click(label: string) { const button = [...document.querySelectorAll('button')].find(el => el.textContent === label); assert.ok(button, label); await act(async () => button.click()); }
test('real space callback uses ID; absent handler is disabled', async () => {
  let id: string | undefined;
  await render(NativeSpacesDirectory, { spaces: [{ id: 'actual-id', title: 'Actual space' }], onOpen: (value: string) => id = value });
  await click('Actual space›'); assert.equal(id, 'actual-id');
  await render(NativeSpacesDirectory, { spaces: [{ id: 'actual-id', title: 'Actual space' }] });
  assert.equal(document.querySelector('button')!.disabled, true);
});
test('null notification capability is not an empty approvals state; tab switch exposes task capability', async () => {
  await render(NativeNotifications, { requests: null, agents: null, mentions: null, items: null });
  assert.match(text(), /Approval requests unavailable/); assert.doesNotMatch(text(), /Nothing needs you/);
  await click('Tasks'); assert.match(text(), /Tasks unavailable/); assert.doesNotMatch(text(), /No open tasks/);
  await click('Approvals');
});
test('approvals call only supplied actions and task counts use actual native task predicate', async () => {
  const approved: string[] = [];
  await render(NativeNotifications, { agents: [], mentions: [], items: [], requests: [{ id: 'request-id', agent: { ...person, kind: 'agent' }, title: 'Actual request', detail: 'Actual detail', reason: '', status: 'pending', audience: 'External', actionLabel: 'Payment', spaceTitle: 'Actual space', costCents: 1234 }], onApprove: (id: string) => approved.push(id) });
  assert.match(text(), /\$12\.34/); await click('Approve'); assert.deepEqual(approved, ['request-id']);
  assert.equal([...document.querySelectorAll('button')].find(el => el.textContent === 'Decline')!.disabled, true);
  const base = { id: 'i', title: 'Item', origin: 'Actual', spaceTitle: 'Space', kindId: 'note' };
  assert.equal(nativeOpenTasks([base, { ...base, id: 'task', kindId: 'task' }, { ...base, id: 'plan', plan: { sections: [{ lines: [{ done: false }] }] } }]).length, 2);
});
test('approvals chip excludes stale requests while the review list retains them', async () => {
  const request = { agent: { ...person, kind: 'agent' }, detail: '', reason: '', audience: '', actionLabel: '', spaceTitle: '' };
  await render(NativeNotifications, { agents: [], mentions: [], items: [], requests: [
    { ...request, id: 'pending', title: 'Pending request', status: 'pending' },
    { ...request, id: 'stale', title: 'Stale request', status: 'stale' },
  ] });
  assert.equal(document.querySelector('[role="tab"][aria-selected="true"]')!.textContent, 'Approvals1');
  assert.match(text(), /Pending request/); assert.match(text(), /Stale request/);
});
test('profile trust labels mirror native labels and literal English Space(s) copy', async () => {
  const agent = { id: 'mine', persona: { ...person, kind: 'agent', isMine: true }, nearLimit: false, spaces: ['suggest', 'listen', 'autonomous', 'act', 'listen'].map((level, i) => ({ spaceId: String(i), spaceTitle: 'Chat', level })) };
  await render(NativeProfile, { me: null, spaces: [], agents: [agent], stats: null, grants: [], account: null, connection: { state: 'offline', synced: false, pending: 0 } });
  assert.match(text(), /Act, Autonomous, Listen, Suggest · 5 Space\(s\)/);
});
test('profile keeps explicit demo/live mode, real stats, owned agents and null capabilities', async () => {
  const props = { me: person, spaces: [{ id: 'space', title: 'Space' }], agents: null, stats: { items: 8, allLogsValid: true }, grants: null, account: null, connection: { state: 'offline', synced: false, pending: 0 } };
  await render(NativeProfile, props);
  assert.match(text(), /Chats sync, signed/); assert.match(text(), /Agent access grants unavailable/); assert.match(text(), /Mini-app access unavailable/); assert.doesNotMatch(text(), /Restart the demo/); assert.doesNotMatch(text(), /Apple/);
  await render(NativeProfile, { ...props, syncMode: 'demo', grants: [], agents: [{ id: 'mine', persona: { ...person, name: 'Owned agent', kind: 'agent', isMine: true }, spaces: [], nearLimit: false }, { id: 'other', persona: { ...person, name: 'Other agent', kind: 'agent', isMine: false }, spaces: [], nearLimit: false }] });
  assert.match(text(), /Everything stays on this device/); assert.match(text(), /Owned agent/); assert.doesNotMatch(text(), /Other agent/); assert.match(text(), /No mini-app has asked/);
  assert.equal([...document.querySelectorAll('button')].find(el => el.textContent === 'Restart the demo')!.disabled, true);
  assert.equal(nativeConnectionLabel({ state: 'offline', synced: false, pending: 3 }), 'Offline · 3 queued');
});
