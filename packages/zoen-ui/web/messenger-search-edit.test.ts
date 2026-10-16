import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

// Mounted production React UI with an in-memory controller, not a relay/MLS E2E journey.
const output = resolve(import.meta.dirname, '../node_modules/.cache/messenger-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'messenger.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic' });
const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost/' });
Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: dom.window.navigator });
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: dom.window.localStorage });
dom.window.HTMLElement.prototype.scrollIntoView = () => {};
const { createElement: h, act } = await import('react');
const { createRoot } = await import('react-dom/client');
const { Messenger, homeSnapshots } = await import(pathToFileURL(output).href);
const own = '11'.repeat(32);
function msg(id: string, text: string) { return { id, entry: id, author: own, name: 'Ana', text, at_ms: 1000, delivery: 'confirmed', failure: null, ticks: 'sent', edited: false, deleted: false, forwarded: false, reactions: [], quote: null, thread: null, replies: 0, repliers: [], last_reply_ms: null }; }
const conversations = [{ id: 'chat', title: 'Amigos', kind: 'group', can_send: true, waiting: null, unreadable: 0, unread: 0, mentions: 0, members: 2, messages: [msg('first', 'pousada antiga'), msg('second', 'Texto para editar')] }];
async function input(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const proto = el.tagName === 'TEXTAREA' ? dom.window.HTMLTextAreaElement.prototype : dom.window.HTMLInputElement.prototype;
  await act(async () => { Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(el, value); el.dispatchEvent(new dom.window.Event('input', { bubbles: true })); });
}
async function click(el: Element | undefined | null) { assert.ok(el); await act(async () => (el as HTMLElement).click()); }
function button(label: string) { return [...document.querySelectorAll('button')].find((b) => b.textContent === label || b.getAttribute('aria-label') === label); }
async function mount(annotate = async () => {}, data = conversations) {
  const calls: unknown[] = [];
  const root = createRoot(document.getElementById('root')!);
  await act(async () => root.render(h(Messenger, { conversations: data, identity: own, name: 'Ana', connected: true, status: 'Conectado', messages: { current: { annotate, post: async (v: unknown) => { calls.push(v); }, sendFile: async (...args: unknown[]) => { calls.push(args); } } }, footer: null, selected: 'chat', select: () => {} })));
  return { calls, close: async () => { await act(async () => root.unmount()); } };
}

test('sidebar finds message bodies, not only conversation titles', async () => {
  const ui = await mount();
  try {
    await click(button('Search'));
    await input(document.querySelector('input[type=search]')!, 'pousada');
    assert.ok(document.querySelector('[aria-label="Resultados de mensagens"]'), 'message results must be visible');
    assert.match(document.querySelector('[aria-label="Resultados de mensagens"]')!.textContent!, /pousada antiga/);
    await click(document.querySelector('[aria-label="Resultados de mensagens"] li button'));
    assert.equal(document.activeElement?.id, 'message-first');
  } finally { await ui.close(); }
});

test('inbox filter selects actual conversation kinds and search opens from the bottom bar', async () => {
  const ui = await mount();
  try {
    assert.equal(document.querySelector('input[type=search]'), null);
    assert.ok(document.querySelector('.nw-empty'));
    const filter = document.querySelector('[aria-label="Filter chats"]') as HTMLSelectElement;
    await act(async () => { filter.value = 'direct'; filter.dispatchEvent(new dom.window.Event('change', { bubbles: true })); });
    assert.equal(document.querySelectorAll('.zm-chats > li').length, 0);
    await act(async () => { filter.value = 'group'; filter.dispatchEvent(new dom.window.Event('change', { bubbles: true })); });
    assert.equal(document.querySelectorAll('.zm-chats > li').length, 1);
    await click(button('Search'));
    assert.ok(document.querySelector('input[type=search]'));
    assert.equal(document.querySelector('.zm-home-head'), null);
  } finally { await ui.close(); }
});

test('successful edit sends the exact replacement once, closes the dialog and preserves the send draft', async () => {
  const annotations: unknown[] = [];
  const ui = await mount(async (...args: unknown[]) => { annotations.push(args); });
  try {
    const send = document.querySelector('.zm-main textarea')! as HTMLTextAreaElement;
    await input(send, 'mensagem futura');
    await click(document.querySelectorAll('[aria-label="Mais ações"]')[1]);
    await click(button('Editar'));
    await input(document.querySelector('[aria-label="Editar mensagem"] textarea')!, 'Nova versão');
    await click(button('Salvar'));
    assert.equal(document.querySelector('[aria-label="Editar mensagem"]'), null);
    assert.deepEqual(annotations, [['chat', 'second', { Edit: { text: 'Nova versão' } }]]);
    assert.equal(send.value, 'mensagem futura');
    assert.deepEqual(ui.calls, []);
  } finally { await ui.close(); }
});

test('delete failure is visible and does not discard the send draft', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  dom.window.confirm = () => true;
  const ui = await mount(async () => { throw new Error('Só quem enviou pode mudar esta mensagem.'); });
  try {
    const send = document.querySelector('.zm-main textarea')! as HTMLTextAreaElement;
    await input(send, 'rascunho preservado');
    await click(document.querySelectorAll('[aria-label="Mais ações"]')[1]);
    await click(button('Apagar para todos'));
    assert.match(document.querySelector('[role=alert]')!.textContent!, /Só quem enviou/);
    assert.equal(send.value, 'rascunho preservado');
    assert.deepEqual(ui.calls, []);
    await act(async () => t.mock.timers.tick(3300));
  } finally { await ui.close(); }
});

test('edit is a separate dialog; refusal retains edit text and cancelling preserves the send draft', async () => {
  const ui = await mount(async () => { throw new Error('Essa mensagem não está mais disponível.'); });
  try {
    const send = document.querySelector('.zm-main textarea')! as HTMLTextAreaElement;
    await input(send, 'rascunho independente');
    await click(document.querySelectorAll('[aria-label="Mais ações"]')[1]);
    await click(button('Editar'));
    const editor = document.querySelector('[role=dialog][aria-label="Editar mensagem"]');
    assert.ok(editor, 'editing must not reuse the send composer');
    await input(editor.querySelector('textarea')!, 'edição preservada');
    await click(button('Salvar'));
    assert.equal(editor.querySelector('textarea')!.value, 'edição preservada');
    assert.match(editor.textContent!, /não está mais disponível/);
    await click(button('Cancelar'));
    assert.equal(send.value, 'rascunho independente');
    assert.deepEqual(ui.calls, []);
  } finally { await ui.close(); }
});


test('reaction journey uses the dialog button, not the retired menuitem', async () => {
  const annotations: unknown[] = [];
  const ui = await mount(async (...args: unknown[]) => { annotations.push(args); });
  try {
    await click(document.querySelector('[aria-label="Reagir"]'));
    const dialog = document.querySelector('[role="dialog"][aria-label="Escolher reação"]')!;
    assert.ok(dialog);
    assert.equal(dialog.querySelector('[role="menuitem"]'), null);
    await click([...dialog.querySelectorAll('button')].find((b) => b.textContent === '❤️'));
    assert.deepEqual(annotations, [['chat', 'first', { React: { emoji: '❤️' } }]]);
  } finally { await ui.close(); }
});

test('voice recording carries the selected inline reply', async (t) => {
  let now = 1000;
  t.mock.method(Date, 'now', () => now);
  const globals = ['MediaRecorder', 'AudioContext', 'requestAnimationFrame', 'cancelAnimationFrame'];
  const descriptors = globals.map((key) => Object.getOwnPropertyDescriptor(globalThis, key));
  class Recorder {
    static isTypeSupported() { return true; }
    mimeType = 'audio/mp4'; state = 'inactive';
    ondataavailable?: (event: { data: Blob }) => void; onstop?: () => void;
    start() { this.state = 'recording'; }
    stop() { this.state = 'inactive'; this.ondataavailable?.({ data: new Blob(['voice']) }); this.onstop?.(); }
  }
  class Context {
    resume() { return Promise.resolve(); } close() { return Promise.resolve(); }
    createAnalyser() { return { fftSize: 0, frequencyBinCount: 128, getByteTimeDomainData: (b: Uint8Array) => b.fill(128) }; }
    createMediaStreamSource() { return { connect() {} }; }
  }
  Object.assign(globalThis, { MediaRecorder: Recorder, AudioContext: Context, requestAnimationFrame: () => 1, cancelAnimationFrame: () => {} });
  Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getUserMedia: async () => ({ getTracks: () => [{ stop() {} }] }) } });
  const ui = await mount();
  try {
    await click(document.querySelector('[aria-label="Responder"]'));
    await click(button('Gravar mensagem de voz'));
    now += 1500;
    await click(button('Enviar mensagem de voz'));
    assert.equal(ui.calls.length, 1);
    const [space, file] = ui.calls[0] as [string, { voice: { reply: unknown } }];
    assert.equal(space, 'chat');
    assert.deepEqual(file.voice.reply, { to: 'first', thread: false });
  } finally {
    await ui.close();
    globals.forEach((key, i) => { const d = descriptors[i]; if (d) Object.defineProperty(globalThis, key, d); else Reflect.deleteProperty(globalThis, key); });
    Reflect.deleteProperty(navigator, 'mediaDevices');
  }
});


test('production Home uses latest snapshots in first appearance order and opens honest details notice', async () => {
  const snapshot = (id: string, title: string) => ({ id, appId: id, title, template: 'stat', accentHex: '#448844', symbol: 'star', deepLink: `zoen://app/${id}`, value: '3' });
  const entry = (id: string, at: number, title: string) => ({ ...msg(`${id}-${at}`, ''), at_ms: at, card: { snapshot: snapshot(id, title) } });
  const data = [{ ...conversations[0], messages: [entry('a', 30, 'Latest A'), entry('b', 20, 'Second B'), entry('a', 10, 'Old A'), { ...entry('c', 5, 'Deleted'), deleted: true }] }];
  assert.deepEqual(homeSnapshots(data).map((s: {title: string}) => s.title), ['Latest A', 'Second B']);
  assert.deepEqual(homeSnapshots(conversations), []);
  const ui = await mount(undefined, data);
  try {
    assert.equal(document.querySelectorAll('.nw-strip .nw-card').length, 2);
    assert.equal(document.querySelector('.nw-empty'), null);
    await click(document.querySelector('.nw-open'));
    assert.match(document.querySelector('[role=dialog]')!.textContent!, /Item details are not available on web yet/);
    await click(button('Close'));
    assert.equal(document.querySelector('[role=dialog]'), null);
  } finally { await ui.close(); }
});
