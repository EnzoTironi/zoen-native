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
async function mount(annotate = async () => {}) {
  const calls: unknown[] = [];
  const root = createRoot(document.getElementById('root')!);
  await act(async () => root.render(h(Messenger, { conversations, identity: own, name: 'Ana', connected: true, status: 'Conectado', messages: { current: { annotate, post: async (v: unknown) => { calls.push(v); } } }, footer: null, selected: 'chat', select: () => {} })));
  return { calls, close: async () => { await act(async () => root.unmount()); } };
}

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
