import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const output = resolve(import.meta.dirname, '../node_modules/.cache/root-voice-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-navigation.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' } });
const dom = new JSDOM('<div id="root"></div>');
Object.assign(globalThis, { window: dom.window, document: dom.window.document, IS_REACT_ACT_ENVIRONMENT: true });
const { createElement: h, act } = await import('react');
const { createRoot } = await import('react-dom/client');
const { NativeNavigation } = await import(pathToFileURL(output).href);
const hookOutput = resolve(import.meta.dirname, '../node_modules/.cache/root-voice-hook-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'root-voice.tsx')], outfile: hookOutput, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' } });
const { useRootVoice, rootVoiceDestination } = await import(pathToFileURL(hookOutput).href);
test('destination is a sendable actual direct Zoen counterpart, not a title match', () => {
  assert.equal(rootVoiceDestination([{ id: 'fake', title: 'Zoen', kind: 'group', can_send: true }]), null);
  assert.equal(rootVoiceDestination([{ id: 'blocked', kind: 'direct', can_send: false, counterpart: { handle: 'zoen' } }]), null);
  assert.equal(rootVoiceDestination([{ id: 'real', kind: 'direct', can_send: true, counterpart: { handle: 'zoen' } }]), 'real');
});
test('null controller honestly rejects accessible record action without opening microphone', async () => {
  const notices: string[] = [];
  const root = createRoot(document.getElementById('root')!);
  function Harness() {
    const voice = useRootVoice({ conversations: [{ id: 'real', kind: 'direct', can_send: true, counterpart: { handle: 'zoen' } }], messages: { current: null }, select() { throw new Error('Must not navigate'); }, onNotice: (s: string) => notices.push(s) });
    return h('button', { onClick: voice.start }, 'Record');
  }
  try {
    await act(async () => root.render(h(Harness)));
    await act(async () => (document.querySelector('button') as HTMLButtonElement).click());
    assert.equal(notices.length, 1);
    assert.match(notices[0]!, /existing connected direct conversation/);
  } finally { await act(async () => root.unmount()); }
});
function pointer(target: EventTarget, type: string, x = 0, y = 0) {
  const event = new dom.window.Event(type, { bubbles: true });
  Object.assign(event, { pointerId: 7, button: 0, clientX: x, clientY: y });
  target.dispatchEvent(event);
}
async function mount() {
  const calls: string[] = [];
  const root = createRoot(document.getElementById('root')!);
  const props = { selected: 'chats', notificationCount: 3, identity: { kind: 'person', id: 'me', name: 'Enzo', initials: 'E', tintHex: '#4F7CFF' },
    onSelect(tab: string) { calls.push(tab); }, onNewMessage() { calls.push('new-message'); },
    voice: { active: false, locked: false, seconds: 0, levels: [], start() { calls.push('start'); }, lock() { calls.push('lock'); }, send() { calls.push('send'); }, cancel() { calls.push('cancel'); } } };
  await act(async () => root.render(h(NativeNavigation, props)));
  return { calls, root, props, close: async () => { await act(async () => root.unmount()); } };
}
// Current native TabView has no root PlusVoiceCapture. Detail composer gesture tests
// remain in voice-hold.test.ts and the real MediaRecorder browser producer.
test('native four tabs dispatch their actual destination IDs without retired Search/Plus controls', async () => {
  const ui = await mount();
  try {
    for (const label of ['Chats', 'Spaces', 'Notifications, 3 pending', 'Profile']) {
      const button = document.querySelector(`[aria-label="${label}"]`) as HTMLButtonElement;
      assert.ok(button); await act(async () => button.click());
    }
    assert.deepEqual(ui.calls, ['chats', 'spaces', 'notifications', 'profile']);
    assert.equal(document.querySelector('.zn-plus, .zn-search-trigger'), null);
  } finally { await ui.close(); }
});
test('New message opens its callback, and a legacy voice prop cannot start a hidden root recorder', async () => {
  const ui = await mount();
  try {
    const compose = document.querySelector('[aria-label="New message"]') as HTMLButtonElement;
    assert.ok(compose);
    await act(async () => { pointer(compose, 'pointerdown'); await new Promise(resolve => setTimeout(resolve, 725)); pointer(compose, 'pointerup'); });
    assert.deepEqual(ui.calls, []);
    await act(async () => compose.click());
    assert.deepEqual(ui.calls, ['new-message']);
  } finally { await ui.close(); }
});
test('navigation shows the supplied pending count and actual profile avatar while keeping selection state', async () => {
  const ui = await mount();
  try {
    assert.ok(document.querySelector('[aria-label="Notifications, 3 pending"]'));
    assert.match(document.querySelector('[aria-label="Profile"]')!.textContent!, /E/);
    await act(async () => ui.root.render(h(NativeNavigation, { ...ui.props, selected: 'profile', notificationCount: 4 })));
    assert.equal(document.querySelector('[aria-current="page"]')?.getAttribute('aria-label'), 'Profile');
    assert.ok(document.querySelector('[aria-label="Notifications, 4 pending"]'));
    assert.deepEqual(ui.calls, []);
  } finally { await ui.close(); }
});
