import test from 'node:test';
import assert from 'node:assert/strict';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import type { CoreOperation } from './worker-client.ts';
import { Messages, type MessagesState } from './messages.ts';
import type { StoredSession } from './session.ts';

// These tests exercise connection/storage ordering. Native protobuf/MLS
// interoperability requires the separate owned native relay fixture journey.
class Socket {
  static readonly OPEN = 1;
  static readonly instances: Socket[] = [];
  readyState = 0;
  binaryType = '';
  bufferedAmount = 0;
  sent: Uint8Array[] = [];
  onopen: (() => void) | undefined;
  onmessage: ((event: { data: ArrayBuffer }) => void) | undefined;
  onclose: ((event: { code: number }) => void) | undefined;
  onerror: (() => void) | undefined;
  readonly url: string;
  constructor(url: string) { this.url = url; Socket.instances.push(this); }
  open() { this.readyState = Socket.OPEN; this.onopen?.(); }
  send(value: Uint8Array) { this.sent.push(Uint8Array.from(value)); }
  frame(value: unknown) { this.onmessage?.({ data: new TextEncoder().encode(JSON.stringify(value)).buffer }); }
  close(code = 1006) { this.readyState = 3; this.onclose?.({ code }); }
}
const login = { identity: '11'.repeat(32), device: '22'.repeat(32), cert: 'cert', relay: 'https://relay.example' };
const challenge = { kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 };
const ready = { kind: 'ready', identity: login.identity, registered: true };
const vault = { sign: async () => new Uint8Array(64) };
function unitCodec() {
  return { bytes: async (operation: CoreOperation) => new TextEncoder().encode(operation.kind),
    text: async (operation: CoreOperation) => {
      if (operation.kind !== 'decode_server') throw new Error('Unexpected test operation.');
      return new TextDecoder().decode(operation.bytes);
    } };
}
function socket() { const value = Socket.instances.at(-1); assert.ok(value); return value; }
function install(t: test.TestContext) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Socket.instances.length = 0;
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  t.mock.timers.enable({ apis: ['setTimeout'] });
  t.after(() => { if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
}
async function settle() { await new Promise<void>((resolve) => setImmediate(resolve)); }
async function authenticate() {
  socket().open(); await settle(); socket().frame(challenge); await settle(); socket().frame(ready); await settle();
}

test('disconnect during Ready FetchLink preserves reconnect', async (t) => {
  install(t);
  const states: ConnectionState[] = [];
  const transport = new AuthenticatedTransport(unitCodec(), vault, login, (state) => states.push(state), async (frame) => {
    if (frame.kind === 'ready') await transport.request('fetch_link', 'box');
  });
  t.after(() => transport.stop());
  transport.start(); await authenticate();
  const first = socket(); assert.equal(first.sent.length, 3);
  first.close(); await settle();
  assert.equal(states.at(-1)?.kind, 'reconnecting');
  assert.equal(states.some((state) => state.kind === 'failed'), false);
  t.mock.timers.tick(900); await settle();
  assert.equal(Socket.instances.length, 2);
  assert.notEqual(socket(), first);
  await authenticate();
  assert.equal(states.at(-1)?.kind, 'ready');
});

test('a delayed Ready decode from a closed generation cannot authenticate its replacement', async (t) => {
  install(t);
  const codec = unitCodec();
  let finish = (_: string) => {};
  const delayed = new Promise<string>((resolve) => { finish = resolve; });
  let delay = true;
  const core = { ...codec, text: async (operation: CoreOperation) => {
    const result = await codec.text(operation);
    if (delay && JSON.parse(result).kind === 'ready') { delay = false; return delayed; }
    return result;
  } };
  const states: ConnectionState[] = [];
  let received = 0;
  const transport = new AuthenticatedTransport(core, vault, login, (state) => states.push(state), async () => { ++received; });
  t.after(() => transport.stop());
  transport.start(); await authenticate(); socket().close();
  t.mock.timers.tick(900); await settle();
  socket().open(); await settle(); socket().frame(challenge); await settle();
  finish(JSON.stringify(ready)); await settle();
  assert.equal(received, 0);
  assert.equal(states.at(-1)?.kind, 'authenticating');
  socket().frame(ready); await settle();
  assert.equal(received, 1);
  assert.equal(states.at(-1)?.kind, 'ready');
});

test('closing a generation while auth bytes await never sends stale authentication', async (t) => {
  install(t);
  const codec = unitCodec();
  let finish = (_: Uint8Array<ArrayBuffer>) => {};
  const delayed = new Promise<Uint8Array<ArrayBuffer>>((resolve) => { finish = resolve; });
  const core = { ...codec, bytes: async (operation: CoreOperation) => operation.kind === 'auth' ? delayed : codec.bytes(operation) };
  const transport = new AuthenticatedTransport(core, vault, login, () => {}, async () => {});
  t.after(() => transport.stop());
  transport.start(); socket().open(); await settle(); socket().frame(challenge); await settle();
  const first = socket(); first.close(); t.mock.timers.tick(900); await settle();
  finish(new Uint8Array([99])); await settle();
  assert.equal(first.sent.length, 1);
  assert.equal(socket().sent.length, 0);
});

test('native early presence does not replace or prevent authenticated Ready', async (t) => {
  install(t);
  const states: ConnectionState[] = [];
  const transport = new AuthenticatedTransport(unitCodec(), vault, login, (state) => states.push(state), async () => {});
  t.after(() => transport.stop());
  transport.start(); socket().open(); await settle(); socket().frame(challenge); await settle();
  socket().frame({ kind: 'other' }); await settle();
  assert.equal(states.at(-1)?.kind, 'authenticating');
  socket().frame(ready); await settle();
  assert.equal(states.at(-1)?.kind, 'ready');
});

const linked: Extract<StoredSession, { kind: 'linked' }> = { kind: 'linked', account: {
  ...login, name: 'Web fixture', handle: 'web_fixture', primary: '33'.repeat(32), primary_hpke: '', hpke_secret: '',
  link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0, profile_fields: '',
}, snapshot: '' };
const peer = '44'.repeat(32);
const anotherPeer = '55'.repeat(32);
const conversation = { id: 'sp_fixture', title: 'Fixture', kind: 'group', can_send: true, waiting: null, unreadable: 0, member_ids: [login.identity, peer, anotherPeer], messages: [] };
function messagingCore() {
  let closed = false;
  return { ...unitCodec(), close() { closed = true; }, get closed() { return closed; },
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'restore') return JSON.stringify([conversation]);
      if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
      if (operation.kind === 'bootstrap') return JSON.stringify({ snapshot: 'durable bootstrap', frames: [[7]], conversations: [conversation] });
      if (operation.kind === 'queue_message') return JSON.stringify({ snapshot: 'new MLS send ratchet', frames: [[8]], conversations: [conversation] });
      if (operation.kind === 'receive') return JSON.stringify({ snapshot: 'received', frames: [], conversations: [conversation] });
      throw new Error('Unexpected test core operation.');
    } };
}

test('production message controller terminates the core and emits nothing when IndexedDB save fails', async (t) => {
  install(t);
  const core = messagingCore();
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async () => { throw new Error('QuotaExceededError'); } },
    changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  assert.equal(socket().sent.length, 2); // hello/auth only; no MLS/bootstrap frame.
  assert.equal(core.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  t.mock.timers.tick(40_000); await settle();
  assert.equal(Socket.instances.length, 1);
});

test('mutation refusals propagate in Portuguese without terminating the session; retry persists before send', async (t) => {
  install(t);
  const base = messagingCore();
  let refusal = 'Only the person who sent this message can change it.';
  let mutationSaves = 0;
  const core = { ...base, text: async (operation: CoreOperation) => {
    if (operation.kind !== 'queue_annotation') return base.text(operation);
    if (refusal) throw new Error(refusal);
    return JSON.stringify({ snapshot: 'mutation', frames: [[9]], conversations: [conversation] });
  } };
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async (saved) => {
    if (saved.kind === 'linked' && saved.snapshot === 'mutation') {
      assert.equal(socket().sent.length, 3);
      ++mutationSaves;
    }
  } }, changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  await assert.rejects(messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } }), /Só quem enviou/);
  assert.equal(states.at(-1)?.kind, 'active');
  assert.equal(base.closed, false);
  assert.equal(socket().sent.length, 3);
  refusal = 'Essa mensagem foi apagada.';
  await assert.rejects(messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } }), /Essa mensagem foi apagada/);
  refusal = 'Write a message of up to 32 KB.';
  await assert.rejects(messages.annotate('sp_fixture', 'target', { Edit: { text: '' } }), /não pode ficar vazia/);
  assert.equal(states.at(-1)?.kind, 'active');
  refusal = '';
  await messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } });
  assert.equal(mutationSaves, 1);
  assert.equal(socket().sent.length, 4);
});

test('mutation persistence failure is fatal and never emits an unsaved annotation', async (t) => {
  install(t);
  const base = messagingCore();
  const core = { ...base, text: async (operation: CoreOperation) => operation.kind === 'queue_annotation'
    ? JSON.stringify({ snapshot: 'mutation', frames: [[9]], conversations: [conversation] }) : base.text(operation) };
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async (saved) => {
    if (saved.kind === 'linked' && saved.snapshot === 'mutation') throw new Error('Storage denied');
  } }, changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  const current = socket();
  await assert.rejects(messages.annotate('sp_fixture', 'target', 'Delete'), /Storage denied/);
  assert.equal(base.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  assert.equal(current.sent.length, 3);
});

test('production send persists the ratchet before emitting and reconnect replays only saved state', async (t) => {
  install(t);
  const core = messagingCore();
  let complete = () => {};
  const saving = new Promise<void>((resolve) => { complete = resolve; });
  const saves: StoredSession[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async (session) => {
    saves.push(session);
    if (session.kind === 'linked' && session.snapshot === 'new MLS send ratchet') await saving;
  } }, changed: () => {} });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  const first = socket(); assert.equal(first.sent.length, 3);
  const sending = messages.send({ space: 'sp_fixture', text: 'Encrypted message' }); await settle();
  assert.equal(first.sent.length, 3);
  first.close(); await settle(); complete(); await sending;
  assert.equal(first.sent.length, 3);
  t.mock.timers.tick(900); await settle(); await authenticate();
  assert.equal(socket().sent.length, 3);
  assert.equal(saves.length, 3);
  assert.equal(core.closed, false);
});

test('production controller trusts only self privacy, tracks concurrent typing, and clears transient state across reconnect', async (t) => {
  install(t);
  const core = messagingCore();
  const operations: CoreOperation[] = [];
  const originalBytes = core.bytes;
  core.bytes = async (op) => { operations.push(op); return originalBytes(op); };
  const states: MessagesState[] = [];
  let saves = 0;
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async () => { ++saves; } }, changed: (s) => states.push(s) });
  const state = () => { const s = states.at(-1); assert.equal(s?.kind, 'active'); if (s?.kind !== 'active') throw new Error('Expected active state'); return s; };
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  assert.equal(state().presence.hideLastSeen, null);
  socket().frame({ kind: 'presence', identity: login.identity, online: true, hide_last_seen: true }); await settle();
  socket().frame({ kind: 'presence', identity: peer, online: false, last_seen_ms: 123, hide_last_seen: false }); await settle();
  assert.equal(state().presence.hideLastSeen, true);
  assert.equal(state().presence.people[peer]?.last_seen_ms, 123);
  for (const from of [peer, anotherPeer, 'not-a-member', login.identity]) {
    socket().frame({ kind: 'ephemeral', space: conversation.id, from, activity: 'typing' }); await settle();
  }
  assert.deepEqual(state().presence.typing[conversation.id], [peer, anotherPeer]);
  socket().frame({ kind: 'ephemeral', space: conversation.id, from: peer, activity: 'stopped_typing' }); await settle();
  assert.deepEqual(state().presence.typing[conversation.id], [anotherPeer]);
  // None clears an old last-seen value; no ephemeral data reaches durable storage.
  socket().frame({ kind: 'presence', identity: peer, online: false, last_seen_ms: null }); await settle();
  assert.equal(state().presence.people[peer]?.last_seen_ms, null);
  assert.equal(saves, 1);
  messages.typing(conversation.id, true); await settle();
  messages.typing(conversation.id, true); await settle();
  assert.equal(operations.filter((op) => op.kind === 'request' && op.op === 'typing').length, 1);
  t.mock.timers.tick(4_000); await settle();
  assert.equal(operations.filter((op) => op.kind === 'request' && op.op === 'stopped_typing').length, 1);
  await messages.setPresencePrivacy(false);
  assert.ok(operations.some((op) => op.kind === 'request' && op.op === 'presence_privacy' && op.value === 'false'));
  // Local request does not invent acknowledgment; self echo changes the control.
  assert.equal(state().presence.hideLastSeen, true);
  socket().frame({ kind: 'presence', identity: login.identity, online: true, hide_last_seen: false }); await settle();
  assert.equal(state().presence.hideLastSeen, false);
  messages.typing(conversation.id, true); await settle();
  socket().close(); await settle();
  assert.deepEqual(state().presence, { people: {}, typing: {}, hideLastSeen: null });
  const sentBefore = operations.length;
  await assert.rejects(messages.setPresencePrivacy(true), /Sem conexão/);
  t.mock.timers.tick(900); await settle(); await authenticate();
  assert.equal(state().connection.kind, 'ready');
  assert.deepEqual(state().presence.typing, {});
  assert.equal(operations.slice(sentBefore).some((op) => op.kind === 'request'), false);
});
