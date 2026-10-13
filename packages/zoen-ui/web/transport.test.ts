import test from 'node:test';
import assert from 'node:assert/strict';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import type { CoreOperation } from './worker-client.ts';
import { Messages, type MessagesState } from './messages.ts';
import { parseSession, type LinkedAccount, type PendingLink, type StoredSession } from './session.ts';
import { Pairing, type PairingState } from './pairing.ts';

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
const conversation = { id: 'sp_fixture', title: 'Fixture', kind: 'group', can_send: true, waiting: null, unreadable: 0, messages: [] };
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

const deviceKey = 'ab'.repeat(32);
const account: LinkedAccount = {
  identity: 'cd'.repeat(32), device: deviceKey, cert: 'cert', name: 'Ana', handle: 'ana',
  relay: 'https://relay.example', primary: 'ef'.repeat(32), primary_hpke: 'hpke', hpke_secret: 'mine',
  link_secret: 'link', agreement_secret: 'agree', profile_key: 'profile', profile_key_version: 1,
  profile_seen_version: 1, profile_fields: '{}',
};
const pendingLink: PendingLink = {
  device: deviceKey, relay: 'https://relay.example', hpke_secret: 'mine', secret: 'link',
  code: 'zoen-link:1:' + '00'.repeat(96), check: '123456', box_id: 'box-1',
};

function recordedCore(ops: CoreOperation[]) {
  return {
    bytes: async (operation: CoreOperation) => {
      ops.push(operation);
      return new TextEncoder().encode(operation.kind === 'request' ? JSON.stringify(operation) : operation.kind);
    },
    text: async (operation: CoreOperation) => {
      ops.push(operation);
      if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
      if (operation.kind === 'complete_link') return JSON.stringify(account);
      throw new Error(`Unexpected test operation ${operation.kind}.`);
    },
  };
}

async function until(ready: () => boolean) {
  const started = Date.now();
  while (!ready()) {
    if (Date.now() - started > 2000) throw new Error('timed out');
    await settle();
  }
}

test('the browser receipts a link only after the account save, and a reload receipts again', async (t) => {
  install(t);
  const ops: CoreOperation[] = [];
  const saves: StoredSession[] = [];
  let releaseSave = () => {};
  const held = new Promise<void>((resolve) => { releaseSave = resolve; });
  const vault = {
    device: async () => deviceKey,
    sign: async () => new Uint8Array(64),
    save: async (session: StoredSession) => {
      saves.push(JSON.parse(JSON.stringify(session)) as StoredSession);
      if (session.kind === 'pending' && session.confirming) await held;
    },
  };
  const states: PairingState[] = [];
  const pairing = new Pairing(vault, recordedCore(ops), (state) => states.push(state));
  t.after(() => pairing.stop());
  await pairing.start('https://relay.example', { kind: 'pending', pending: pendingLink });
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'fetch_link'));
  const fetch = ops.find((op) => op.kind === 'request' && op.op === 'fetch_link');
  assert.ok(fetch && fetch.kind === 'request');
  socket().frame({ kind: 'response', id: fetch.requestId, value: { kind: 'link', sealed: [9] } });
  await until(() => saves.some((session) => session.kind === 'pending' && session.confirming !== undefined));
  assert.equal(ops.some((op) => op.kind === 'request' && op.op === 'ack_link'), false);
  const confirming = saves.find((session) => session.kind === 'pending' && session.confirming);
  assert.ok(confirming);
  const reloaded = parseSession(JSON.stringify(confirming));
  assert.equal(reloaded.kind, 'pending');
  if (reloaded.kind === 'pending') assert.deepEqual(reloaded.confirming, account);
  releaseSave();
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'ack_link'));
  const receipt = ops.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(receipt && receipt.kind === 'request');
  assert.equal(receipt.value, pendingLink.box_id);
  socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
  await until(() => states.at(-1)?.kind === 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');

  const again: CoreOperation[] = [];
  const resumed: PairingState[] = [];
  const second = new Pairing(
    { ...vault, save: async (session: StoredSession) => { saves.push(session); } },
    recordedCore(again),
    (state) => resumed.push(state));
  t.after(() => second.stop());
  await second.start('https://relay.example', reloaded);
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => again.some((op) => op.kind === 'request' && op.op === 'ack_link'));
  assert.equal(again.some((op) => op.kind === 'complete_link' || (op.kind === 'request' && op.op === 'fetch_link')), false);
  const retry = again.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(retry && retry.kind === 'request');
  socket().frame({ kind: 'response', id: retry.requestId, value: { kind: 'done' } });
  await until(() => resumed.at(-1)?.kind === 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');
});
