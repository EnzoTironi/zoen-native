import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages } from './messages.ts';
import { parsePage } from './page-data.ts';
import type { CoreOperation } from './worker-client.ts';
import type { StoredSession } from './session.ts';
const page = { item_id: 'page', space_id: 'space', title: 'Shared page', path: 'page.md', version: 1, ready: true, unsaved: false, pending_sync: false, save_error: null, can_edit: true, content_hash: 'hash', edit_context: 'frontiers', blocks: [{ id: 'paragraph', kind: 'paragraph', level: 0, indent: 0, number: 0, checked: false, lang: '', url: '', alt: '', text: 'Hello', spans: [] }] };
test('page results validate native formatting ranges and block identity', () => {
  assert.deepEqual(parsePage(page), page);
  assert.throws(() => parsePage({ ...page, blocks: [...page.blocks, page.blocks[0]] }), /Duplicate/);
  assert.throws(() => parsePage({ ...page, blocks: [{ ...page.blocks[0], spans: [{ start: 0, end: 99, key: 'bold', value: 'true' }] }] }), /formatting range/);
});
class Socket {
  static readonly OPEN = 1; static current: Socket; static flow: string[] = [];
  readyState = 0; onopen?: () => void; onmessage?: (event: { data: ArrayBuffer }) => void; onclose?: (event: { code: number }) => void;
  constructor() { Socket.current = this; }
  send() { Socket.flow.push('send'); } close() { this.readyState = 3; }
  frame(value: unknown) { this.onmessage?.({ data: new TextEncoder().encode(JSON.stringify(value)).buffer }); }
}
const identity = '11'.repeat(32);
const session: Extract<StoredSession, { kind: 'linked' }> = { kind: 'linked', snapshot: '', account: { identity, device: '33'.repeat(32), cert: 'cert', name: 'Alex', handle: 'alex', relay: 'https://relay.example', primary: identity, primary_hpke: '', hpke_secret: '', link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0, profile_fields: '' } };
const conversation = { id: 'space', title: 'Space', kind: 'community', can_send: true, waiting: null, unreadable: 0, messages: [] };
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));
test('real page adapter persists signed update before emitting frames, then reads authoritative page', async (t) => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  let reject = false; let closed = false;
  const messages = new Messages({ session, changed: () => {}, vault: { sign: async () => new Uint8Array(64), save: async () => { Socket.flow.push('persist'); } }, core: { close() { closed = true; }, bytes: async () => new Uint8Array([1]), text: async (operation: CoreOperation) => {
    if (operation.kind === 'restore') return JSON.stringify([conversation]);
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === 'bootstrap') return JSON.stringify({ snapshot: 'initial', frames: [], conversations: [conversation] });
    if (operation.kind === 'page_download_manifest') return JSON.stringify({ ready: true, chunks: [] });
    if (operation.kind === 'page') { Socket.flow.push('read'); return JSON.stringify(page); }
    if (operation.kind === 'page_apply') { Socket.flow.push('apply'); if (reject) throw Error('Core refused'); const request = JSON.parse(operation.request); assert.equal(request.edit_context, 'frontiers'); return JSON.stringify({ snapshot: 'signed-update', frames: [[99]], conversations: [conversation] }); }
    throw Error('Unexpected operation');
  } } });
  t.after(() => { messages.stop(); if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
  await messages.start(); const socket = Socket.current; socket.readyState = Socket.OPEN; socket.onopen?.(); await settle(); socket.frame({ kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 }); await settle(); socket.frame({ kind: 'ready', identity, registered: true }); await settle();
  Socket.flow = [];
  const request = { item_id: 'page', edit_id: 'edit', edit_context: 'frontiers', base_blocks: page.blocks, blocks: page.blocks, order: ['paragraph'], changed: page.blocks };
  assert.deepEqual(await messages.savePage(request, 'space'), page);
  assert.deepEqual(Socket.flow, ['apply', 'persist', 'send', 'read']);
  Socket.flow = []; reject = true;
  await assert.rejects(messages.savePage(request, 'space'), /Não deu para fazer isso agora/);
  assert.deepEqual(Socket.flow, ['apply']); assert.equal(closed, false);
});
