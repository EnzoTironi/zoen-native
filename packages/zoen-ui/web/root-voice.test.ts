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
  const props = { selected: 'chats', search: { open: false, value: '', onChange() {}, onOpenChange() {} }, onSelect() {}, onCreate() {}, voice: { active: false, locked: false, seconds: 0, levels: [], start() { calls.push('start'); }, lock() { calls.push('lock'); }, send() { calls.push('send'); }, cancel() { calls.push('cancel'); } } };
  await act(async () => root.render(h(NativeNavigation, props)));
  const plus = document.querySelector('.zn-plus')!;
  return { calls, plus, root, props, close: async () => { await act(async () => root.unmount()); } };
}
const arm = async (plus: Element) => { await act(async () => { pointer(plus, 'pointerdown'); await new Promise(r => setTimeout(r, 725)); }); };
test('700ms hold sends once on release even after trigger DOM replacement', async () => {
  const ui = await mount();
  try {
    await arm(ui.plus);
    await act(async () => ui.root.render(h(NativeNavigation, { ...ui.props, voice: { ...ui.props.voice, active: true } })));
    await act(async () => { pointer(window, 'pointerup'); pointer(window, 'pointerup'); });
    assert.deepEqual(ui.calls, ['start', 'send']);
  } finally { await ui.close(); }
});
test('leaving center before arming cancels recording arm', async () => {
  const ui = await mount();
  try {
    await act(async () => { pointer(ui.plus, 'pointerdown'); pointer(window, 'pointermove', 15); await new Promise(r => setTimeout(r, 725)); pointer(window, 'pointerup'); });
    assert.deepEqual(ui.calls, []);
  } finally { await ui.close(); }
});
test('left drag cancels once; upward drag locks and release does not send', async () => {
  const ui = await mount();
  try {
    await arm(ui.plus);
    await act(async () => { pointer(window, 'pointermove', -111); pointer(window, 'pointerup'); });
    assert.deepEqual(ui.calls, ['start', 'cancel']);
    ui.calls.length = 0;
    await arm(ui.plus);
    await act(async () => { pointer(window, 'pointermove', 0, -81); pointer(window, 'pointerup'); });
    assert.deepEqual(ui.calls, ['start', 'lock']);
  } finally { await ui.close(); }
});
