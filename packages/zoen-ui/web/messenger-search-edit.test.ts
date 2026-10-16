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
async function mount(annotate = async () => {}, data = conversations, selected = 'chat') {
  const calls: unknown[] = [];
  const root = createRoot(document.getElementById('root')!);
  await act(async () => root.render(h(Messenger, { conversations: data, identity: own, name: 'Ana', connected: true, status: 'Conectado', messages: { current: { annotate, post: async (v: unknown) => { calls.push(v); }, sendFile: async (...args: unknown[]) => { calls.push(args); } } }, footer: null, selected, select: () => {} })));
  return { calls, close: async () => { await act(async () => root.unmount()); } };
}

test('main unread anchor survives markRead and follows only own appends or incoming at bottom', async () => {
  const scrolled: { el: HTMLElement; options: ScrollIntoViewOptions }[] = [];
  dom.window.HTMLElement.prototype.scrollIntoView = function(options) { scrolled.push({ el: this, options: options as ScrollIntoViewOptions }); };
  const incoming = (id: string) => ({ ...msg(id, id), author: 'other' });
  let data = [{ ...conversations[0], unread: 2, messages: [incoming('older'), incoming('unread'), { ...incoming('thread'), thread: 'older' }] }];
  const root = createRoot(document.getElementById('root')!);
  const render = async () => { await act(async () => root.render(h(Messenger, { conversations: data, identity: own, name: 'Ana', connected: true, status: 'Connected', messages: { current: { markRead: async () => {} } }, footer: null, selected: 'chat', select: () => {} }))); };
  try {
    await render();
    const divider = document.querySelector('.zm-unread-divider')!;
    assert.ok(divider);
    assert.equal(divider.nextElementSibling?.id, 'message-unread');
    assert.equal(scrolled[0]?.el, divider);
    assert.equal(scrolled[0]?.options.block, 'start');
    const log = document.querySelector('.zm-main [role=log]') as HTMLElement;
    Object.defineProperties(log, { scrollHeight: { configurable: true, value: 1000 }, clientHeight: { configurable: true, value: 200 }, scrollTop: { configurable: true, writable: true, value: 100 } });
    await act(async () => log.dispatchEvent(new dom.window.Event('scroll', { bubbles: true })));
    scrolled.length = 0;
    data = [{ ...data[0], unread: 0, messages: [...data[0].messages, incoming('new')] }];
    await render();
    assert.ok(document.querySelector('.zm-unread-divider'));
    assert.equal(scrolled.length, 0, 'incoming does not steal a reader’s position');
    data = [{ ...data[0], messages: [...data[0].messages, msg('mine', 'sent')] }];
    await render();
    assert.equal(scrolled.at(-1)?.options.block, 'end', 'own append always follows');
    log.scrollTop = 800;
    await act(async () => log.dispatchEvent(new dom.window.Event('scroll', { bubbles: true })));
    scrolled.length = 0;
    data = [{ ...data[0], messages: [...data[0].messages, incoming('new-bottom')] }];
    await render();
    assert.equal(scrolled.at(-1)?.options.block, 'end');
  } finally { await act(async () => root.unmount()); dom.window.HTMLElement.prototype.scrollIntoView = () => {}; }
});

test('read conversation initially scrolls to bottom without an unread divider', async () => {
  const blocks: unknown[] = [];
  dom.window.HTMLElement.prototype.scrollIntoView = options => { blocks.push((options as ScrollIntoViewOptions)?.block); };
  const ui = await mount();
  try {
    assert.equal(document.querySelector('.zm-unread-divider'), null);
    assert.deepEqual(blocks, ['end']);
  } finally { await ui.close(); dom.window.HTMLElement.prototype.scrollIntoView = () => {}; }
});

test('native four-tab routing opens production directories and profile without retired header controls', async () => {
  const ui = await mount(undefined, conversations, '');
  try {
    assert.equal(document.querySelector('.zm-home-head img'), null);
    assert.equal(button('Search'), undefined);
    await click(button('Spaces'));
    assert.ok(document.querySelector('[data-testid="spaces-directory"]'));
    assert.match(document.querySelector('[data-testid="spaces-directory"]')!.textContent!, /Amigos/);
    await click(button('Profile'));
    assert.ok(document.querySelector('.zm-main [aria-label="Profile"]'));
    assert.match(document.querySelector('.zm-main')!.textContent!, /Your personal space/);
    assert.equal(document.querySelector('[aria-label="Main navigation"] [aria-current="page"]')?.getAttribute('aria-label'), 'Profile');
  } finally { await ui.close(); }
});

test('inbox filter preserves actual conversation kinds and New message opens the production recipient sheet', async () => {
  const ui = await mount(undefined, conversations, '');
  try {
    const search = document.querySelector('input[aria-label="Search chats"]') as HTMLInputElement;
    assert.ok(search);
    assert.ok(document.querySelector('.nw-empty'));
    assert.equal(document.querySelector('.chat-inbox-search select')?.getAttribute('aria-label'), 'Filter chats');
    await input(search, 'no matching chat');
    assert.equal(document.querySelectorAll('.zm-chats > li').length, 0);
    assert.equal(document.querySelector('.nw-empty'), null, 'query hides native home strip');
    assert.equal(document.querySelector('.chat-inbox-search')?.getAttribute('data-compact'), 'true');
    await click(button('Clear search'));
    assert.equal(search.value, '');
    assert.ok(document.querySelector('.nw-empty'));
    const filter = document.querySelector('[aria-label="Filter chats"]') as HTMLSelectElement;
    await act(async () => { filter.value = 'direct'; filter.dispatchEvent(new dom.window.Event('change', { bubbles: true })); });
    assert.equal(document.querySelectorAll('.zm-chats > li').length, 0);
    await act(async () => { filter.value = 'group'; filter.dispatchEvent(new dom.window.Event('change', { bubbles: true })); });
    assert.equal(document.querySelectorAll('.zm-chats > li').length, 1);
    await click(button('New message'));
    const dialog = document.querySelector('[role="dialog"][aria-labelledby="nnm-title"]');
    assert.ok(dialog);
    assert.equal(dialog.querySelector('#nnm-title')?.textContent, 'New chat');
    assert.equal(document.querySelector('.zm-home-head'), null, 'native root has no top header');
    assert.deepEqual(ui.calls, [], 'opening the recipient picker never fabricates a sent message');
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
    // Window release survives the mic being replaced by the recording row.
    const pointer = async (target: EventTarget, type: string, x: number, y: number) => {
      await act(async () => {
        const event = new window.MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 });
        Object.defineProperty(event, 'pointerId', { value: 1 }); target.dispatchEvent(event);
      });
    };
    for (const mode of ['send', 'cancel', 'lock']) {
      await pointer(button('Gravar mensagem de voz')!, 'pointerdown', 200, 200);
      await act(async () => { await new Promise(resolve => setTimeout(resolve, 250)); });
      now += 1500;
      if (mode !== 'send') await pointer(window, 'pointermove', mode === 'cancel' ? 89 : 200, mode === 'lock' ? 119 : 200);
      await pointer(window, 'pointerup', 200, 200);
      if (mode === 'lock') {
        assert.ok(button('Enviar mensagem de voz'), 'locked recorder survives release');
        await click(button('Descartar gravação'));
      }
    }
    assert.equal(ui.calls.length, 2, 'only the unlocked release adds a voice upload');
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
