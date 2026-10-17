import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages } from './messages.ts';
import { parseSession, type StoredSession } from './session.ts';
import type { CoreOperation } from './worker-client.ts';
const identity = '11'.repeat(32);
const account = { identity, device: '33'.repeat(32), cert: 'cert', name: 'Alex', handle: 'alex', relay: 'https://relay.example', primary: identity, primary_hpke: '', hpke_secret: '', link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0, profile_fields: '' };
const initial: Extract<StoredSession, { kind: 'linked' }> = { kind: 'linked', account, snapshot: 'initial' };
const conversation = { id: 'space', title: 'Space', kind: 'community', can_send: true, waiting: null, unreadable: 0, messages: [] };
class Socket {
  static OPEN = 1; static current: Socket; static flow: string[] = [];
  readyState = 0; onopen?: () => void; onmessage?: (event: { data: ArrayBuffer }) => void; onclose?: (event: { code: number }) => void;
  constructor() { Socket.current = this; }
  send() { Socket.flow.push('send'); } close() { this.readyState = 3; }
  frame(value: unknown) { this.onmessage?.({ data: new TextEncoder().encode(JSON.stringify(value)).buffer }); }
}
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));
async function connect(messages: Messages) { await messages.start(); const socket = Socket.current; socket.readyState = 1; socket.onopen?.(); await settle(); socket.frame({ kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 }); await settle(); socket.frame({ kind: 'ready', identity, registered: true }); await settle(); }
function setup(t: test.TestContext, refusePlan = false) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket'); Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  let saved: Extract<StoredSession, { kind: 'linked' }> = initial; let failUpload = true; let puts = 0; const commits: string[] = [];
  const hash = 'a'.repeat(64); const plan = { schema: 1, space: 'space', item: 'file', doc: { name: 'test.txt', mime: 'text/plain', bytes: 1, sha256: hash, chunks: [{ blob: hash, bytes: 1, key: hash, sha256: hash }] }, uploads: [{ blob: hash, body_base64: Buffer.alloc(41, 1).toString('base64'), headers: { 'x-zoen-device': 'device', 'x-zoen-ts': '1', 'x-zoen-sig': 'signature' } }] };
  t.mock.method(globalThis, 'fetch', async () => { puts++; Socket.flow.push('put'); if (failUpload) throw Error('network down'); return new Response(null, { status: 200 }); });
  const controllers: Messages[] = [];
  function create(session = saved) {
    const messages = new Messages({ session, changed: () => {}, vault: { sign: async () => new Uint8Array(64), save: async (next) => { if (refusePlan && next.kind === 'linked' && next.pendingUploads?.length) throw Error('vault unavailable'); const parsed = parseSession(JSON.stringify(next)); assert.equal(parsed.kind, 'linked'); if (parsed.kind === 'linked') saved = parsed; Socket.flow.push(next.kind === 'linked' && next.pendingUploads?.length ? 'persist-plan' : 'persist-snapshot'); } }, core: { close() {}, bytes: async () => new Uint8Array([1]), text: async (op: CoreOperation) => {
      if (op.kind === 'restore') return JSON.stringify([conversation]);
      if (op.kind === 'decode_server') return new TextDecoder().decode(op.bytes);
      if (op.kind === 'bootstrap') return JSON.stringify({ snapshot: session.snapshot, frames: [], conversations: [conversation] });
      if (op.kind === 'file_prepare') return JSON.stringify(plan);
      if (op.kind === 'file_upload_headers') return JSON.stringify({ 'x-zoen-device': 'device', 'x-zoen-ts': String(op.now), 'x-zoen-sig': 'signature' });
      if (op.kind === 'file_commit') { commits.push(op.editId); Socket.flow.push('commit'); return JSON.stringify({ snapshot: 'file-committed', frames: [[99]], conversations: [conversation] }); }
      throw Error('Unexpected operation');
    } } }); controllers.push(messages); return messages;
  }
  t.after(() => { for (const controller of controllers) controller.stop(); if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
  return { create, saved: () => saved, puts: () => puts, commits, allowUploads: () => { failUpload = false; } };
}
test('failed upload retains encrypted-vault plan; restart resumes same edit identity before publishing metadata', async (t) => {
  const ui = setup(t); const first = ui.create(); await connect(first); Socket.flow = [];
  await assert.rejects(first.uploadFile('space', new File(['x'], 'test.txt', { type: 'text/plain' })));
  assert.deepEqual(Socket.flow, ['persist-plan', 'put']); assert.equal(ui.commits.length, 0);
  const stored = ui.saved(); assert.equal(stored.pendingUploads?.length, 1); const editId = stored.pendingUploads![0]!.editId;
  first.stop(); ui.allowUploads(); Socket.flow = []; const second = ui.create(stored); await connect(second);
  // Network fetch/supply crosses promises after the transport's ready callback.
  for (let i = 0; i < 10 && second.pendingUploadCount; i++) await settle();
  assert.equal(second.pendingUploadCount, 0); assert.deepEqual(ui.commits, [editId]);
  assert.equal(ui.saved().snapshot, 'file-committed'); assert.equal(ui.saved().pendingUploads, undefined);
  assert.ok(Socket.flow.indexOf('persist-snapshot') < Socket.flow.lastIndexOf('send'));
});
test('vault refusal prevents every blob PUT and metadata commit', async (t) => {
  const ui = setup(t, true); const messages = ui.create(); await connect(messages);
  await assert.rejects(messages.uploadFile('space', new File(['x'], 'test.txt', { type: 'text/plain' })));
  assert.equal(ui.puts(), 0); assert.equal(ui.commits.length, 0);
});
