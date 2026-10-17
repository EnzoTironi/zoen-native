import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

// Mounted production React UI with an in-memory controller, not a relay/MLS E2E journey.
const output = resolve(import.meta.dirname, '../node_modules/.cache/messenger-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'messenger.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic' });
const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost/', pretendToBeVisual: true });
Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: dom.window.navigator });
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: dom.window.localStorage });
dom.window.HTMLElement.prototype.scrollIntoView = () => {};
const { createElement: h, act } = await import('react');
const { createRoot } = await import('react-dom/client');
const { Messenger } = await import(pathToFileURL(output).href);
const own = '11'.repeat(32);
function msg(id: string, text: string) { return { id, entry: id, author: own, name: 'Ana', text, at_ms: 1000, delivery: 'confirmed', failure: null, ticks: 'sent', edited: false, deleted: false, forwarded: false, reactions: [], quote: null, thread: null, replies: 0, repliers: [], last_reply_ms: null, thread_following: false, thread_unread: 0, thread_mentions: 0, also_in_chat: false }; }
const conversations = [{ id: 'chat', title: 'Amigos', kind: 'group', can_send: true, waiting: null, unreadable: 0, unread: 0, mentions: 0, members: 2, messages: [msg('first', 'pousada antiga'), msg('second', 'Texto para editar')] }];
async function input(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const proto = el.tagName === 'TEXTAREA' ? dom.window.HTMLTextAreaElement.prototype : dom.window.HTMLInputElement.prototype;
  await act(async () => { Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(el, value); el.dispatchEvent(new dom.window.Event('input', { bubbles: true })); });
}
async function click(el: Element | undefined | null) { assert.ok(el); await act(async () => (el as HTMLElement).click()); }
function button(label: string) { return [...document.querySelectorAll('button')].find((b) => b.textContent === label || b.getAttribute('aria-label') === label); }
async function mount(annotate = async () => {}, items: unknown[] = conversations) {
  const calls: unknown[] = [];
  const root = createRoot(document.getElementById('root')!);
  await act(async () => root.render(h(Messenger, { conversations: items, identity: own, name: 'Ana', connected: true, status: 'Conectado', messages: { current: { annotate, post: async (v: unknown) => { calls.push(v); }, sendFile: async (...args: unknown[]) => { calls.push(args); } } }, footer: null, selected: 'chat', select: () => {} })));
  return { calls, close: async () => { await act(async () => root.unmount()); } };
}

test('attachment post renders file payload and caption alongside forward and reply context', async () => {
  const file = { name: 'report.txt', mime: 'text/plain', bytes: 6, chunks: [{ blob: 'blob-id', bytes: 6 }], key: 'key' };
  const entry = { ...msg('attachment', 'Meaningful caption'), file, forwarded: true, quote: { id: 'root', name: 'Bea', text: 'Original question' } };
  const ui = await mount(undefined, [{ ...conversations[0], messages: [entry] }]);
  try {
    const row = document.getElementById('message-attachment')!;
    assert.ok(row.querySelector('.zm-file'), 'actual file component must render');
    assert.match(row.textContent!, /report.txt/);
    assert.equal(row.querySelector('.zm-text')?.textContent, 'Meaningful caption');
    assert.match(row.textContent!, /Original question/);
    assert.match(row.textContent!, /Encaminhada/);
  } finally { await ui.close(); }
});

test('sidebar finds message bodies, not only conversation titles', async () => {
  const ui = await mount();
  try {
    await input(document.querySelector('input[type=search]')!, 'pousada');
    assert.ok(document.querySelector('[aria-label="Resultados de mensagens"]'), 'message results must be visible');
    assert.match(document.querySelector('[aria-label="Resultados de mensagens"]')!.textContent!, /pousada antiga/);
    await click(document.querySelector('[aria-label="Resultados de mensagens"] li button'));
    assert.equal(document.activeElement?.id, 'message-first');
  } finally { await ui.close(); }
});

test('merged replies inbox keeps synced state, message search, broadcasts and account isolation', async () => {
  const calls: unknown[] = [];
  const rootMessage = { ...msg('root', 'Passeio combinado'), replies: 1, thread_following: true, thread_unread: 1, thread_mentions: 1 };
  const replyMessage = { ...msg('reply', 'pousada na serra'), thread: 'root' };
  const chats = [{ ...conversations[0]!, threads_unread: 1, messages: [rootMessage, replyMessage] }];
  const controller = { current: {
    post: async (value: unknown) => { calls.push(value); },
    setThreadFollow: async (...args: unknown[]) => { calls.push(['follow', ...args]); },
    markThreadRead: async (...args: unknown[]) => { calls.push(['read', ...args]); },
  } };
  const root = createRoot(document.getElementById('root')!);
  const render = async (identity = own) => { await act(async () => root.render(h(Messenger, {
    conversations: chats, identity, name: 'Ana', connected: true, status: 'Conectado',
    messages: controller, footer: null, selected: 'chat', select: () => {},
  }))); };
  const originalFocus = document.hasFocus;
  document.hasFocus = () => true;
  try {
    await render();
    assert.equal(document.querySelector('.zm-main #message-reply'), null, 'thread-only replies stay out of chat');
    await click(document.querySelector('button.zm-threads-row'));
    assert.ok(document.querySelector('[aria-label="1 menções não lidas"]'));
    await click(document.querySelector('ul[aria-label="Respostas que você acompanha"] .zm-chat-open'));
    assert.ok(document.querySelector('.zm-thread #message-reply'));
    assert.deepEqual(calls.shift(), ['read', 'chat', 'root']);
    await click(button('Acompanhando'));
    assert.deepEqual(calls.shift(), ['follow', 'chat', 'root', false]);
    await click(document.querySelector('.zm-thread input[type=checkbox]'));
    await input(document.querySelector('.zm-thread textarea')!, 'também na conversa');
    await click(document.querySelector('.zm-thread [aria-label=Enviar]'));
    assert.deepEqual(calls.shift(), { space: 'chat', text: 'também na conversa', replyTo: 'root', thread: true, alsoInChat: true });
    await click(button('Fechar respostas'));
    await input(document.querySelector('input[type=search]')!, 'pousada');
    await click(document.querySelector('[aria-label="Resultados de mensagens"] li button'));
    assert.equal(document.activeElement?.id, 'message-reply', 'search still opens a thread-only result');
    await input(document.querySelector('.zm-main textarea')!, 'rascunho privado');
    await render('22'.repeat(32));
    assert.equal((document.querySelector('.zm-main textarea') as HTMLTextAreaElement).value, '');
    assert.equal(document.querySelector('.zm-thread'), null, 'account switch discards thread selection');
    assert.equal((document.querySelector('input[type=search]') as HTMLInputElement).value, '');
  } finally {
    document.hasFocus = originalFocus;
    await act(async () => root.unmount());
  }
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
