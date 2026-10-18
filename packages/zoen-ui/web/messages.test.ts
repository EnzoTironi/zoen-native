import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages, parseConversation, parseDevices, type MessagesState } from './messages.ts';
import type { CoreOperation } from './worker-client.ts';
import type { StoredSession } from './session.ts';

class Socket {
  static readonly OPEN = 1;
  static current: Socket;
  readyState = 0;
  onopen?: () => void;
  onmessage?: (event: { data: ArrayBuffer }) => void;
  onclose?: (event: { code: number }) => void;
  constructor() { Socket.current = this; }
  send() {}
  close() { this.readyState = 3; }
  frame(value: unknown) { this.onmessage?.({ data: new TextEncoder().encode(JSON.stringify(value)).buffer }); }
}
const identity = '11'.repeat(32);
const other = '22'.repeat(32);
const session: Extract<StoredSession, { kind: 'linked' }> = { kind: 'linked', snapshot: '', account: {
  identity, device: '33'.repeat(32), cert: 'cert', name: 'Eu', handle: 'eu', relay: 'https://relay.example', primary: identity,
  primary_hpke: '', hpke_secret: '', link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0, profile_fields: '',
} };
const conversation = { id: 'sp_fixture', title: 'Conversa', kind: 'group', privacy: 'EndToEnd', roster: {}, profiles: [], prefs: { pinned: false, archived: false, muted_until_ms: null }, event_count: '0', can_send: true, waiting: null, unreadable: 0, unread: 1,
  messages: [{ id: 'message', entry: 'entry', author: other, name: 'Ana', text: 'Oi', at_ms: 1, delivery: 'confirmed', failure: null,
    reactions: [{ emoji: '🧑🏽‍🚀', count: 2, mine: true, participants: [{ identity, name: 'Eu' }, { identity: other, name: 'Ana' }] }] }] };
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));
function setup(t: test.TestContext, read: () => Promise<string> = async () => { throw new Error('Core refused'); }, audit: () => Promise<string> = read) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  const states: MessagesState[] = [];
  let closed = false;
  let saves = 0;
  const core = { close() { closed = true; }, bytes: async () => new Uint8Array([1]), text: async (operation: CoreOperation) => {
    if (operation.kind === 'document_uploads') return '[]';
    if(operation.kind==='claim_clock')return '';
    if (operation.kind === 'restore') return JSON.stringify([conversation]);
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === 'bootstrap') return JSON.stringify({ snapshot: 'snapshot', frames: [], conversations: [conversation] });
    if (operation.kind === 'mark_read') return read();
    if (operation.kind === 'integrity_request') return audit();
    throw new Error('Unexpected operation');
  } };
  const messages = new Messages({ core, session, vault: { save: async () => { saves++; }, sign: async () => new Uint8Array(64) }, changed: (state) => states.push(state) });
  t.after(() => { messages.stop(); if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
  return { messages, states, closed: () => closed, saves: () => saves };
}
async function connect(messages: Messages) {
  await messages.start();
  const socket = Socket.current;
  socket.readyState = Socket.OPEN; socket.onopen?.(); await settle();
  socket.frame({ kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 }); await settle();
  socket.frame({ kind: 'ready', identity, registered: true }); await settle();
}

test('message snapshots preserve reaction participant identities and compound emoji', async (t) => {
  const { messages, states } = setup(t);
  await connect(messages);
  const state = states.at(-1);
  assert.equal(state?.kind, 'active');
  if (state?.kind !== 'active') return;
  assert.deepEqual(state.conversations[0]?.messages[0]?.reactions, conversation.messages[0]!.reactions);
});

test('conversation projections preserve real privacy, roles, profiles and exact preference values', () => {
  for (const privacy of ['EndToEnd', 'Closed', 'Public']) {
    const parsed = parseConversation({ ...conversation, privacy, roster: { [identity]: 'Owner', [other]: 'Reader' },
      profiles: [{ id: other, name: 'Ana', handle: 'ana', kind: 'Agent', owner: identity, tint_hex: '#008800', glyph: 'leaf', bio: 'Assistente' }],
      prefs: { pinned: true, archived: true, muted_until_ms: '9223372036854775807' }, event_count: '18446744073709551615' });
    assert.equal(parsed.privacy, privacy);
    assert.deepEqual(parsed.roster, [{ id: identity, role: 'Owner' }, { id: other, role: 'Reader' }]);
    assert.equal(parsed.profiles[0]?.kind, 'Agent');
    assert.equal(parsed.profiles[0]?.owner, identity);
    assert.equal(parsed.prefs.muted_until_ms, '9223372036854775807');
    assert.equal(parsed.event_count, '18446744073709551615');
  }
  assert.throws(() => parseConversation({ ...conversation, privacy: 'Unknown' }), /privacy/);
  assert.throws(() => parseConversation({ ...conversation, privacy: undefined }), /text/);
  assert.throws(() => parseConversation({ ...conversation, prefs: { pinned: false, archived: false, muted_until_ms: Number('9223372036854775807') } }), /text/);
  assert.throws(() => parseConversation({ ...conversation, event_count: '18446744073709551616' }), /integer/);
});

test('read failures reject in Portuguese without closing the core, and can be retried', async (t) => {
  let attempts = 0;
  const { messages, closed } = setup(t, async () => {
    if (++attempts === 1) throw new Error('Core refused');
    return JSON.stringify({ snapshot: 'read', frames: [], conversations: [{ ...conversation, unread: 0 }] });
  });
  await connect(messages);
  await assert.rejects(messages.markRead('sp_fixture'), /Não deu para marcar a conversa como lida/);
  assert.equal(closed(), false);
  await messages.markRead('sp_fixture');
  assert.equal(attempts, 2);
});

test('device projections identify this device and preserve server revocations', () => {
  assert.deepEqual(parseDevices({ kind: 'devices', devices: [{ device: identity, revoked: false }, { device: other, revoked: true }] }, identity),
    [{ device_id: identity, revoked: false, this_device: true }, { device_id: other, revoked: true, this_device: false }]);
  assert.throws(() => parseDevices({ kind: 'done' }, identity));
  assert.throws(() => parseDevices({ kind: 'devices', devices: [{ device: identity, revoked: 'false' }] }, identity));
});

// An unavailable report is a read failure, not a failed browser account.
test('integrity refusal is nonfatal and read retries never persist or publish a snapshot', async (t) => {
  let attempts = 0;
  const { messages, closed, saves } = setup(t, undefined, async () => {
    if (++attempts === 1) throw new Error('Invalid audit cursor.');
    return JSON.stringify({ valid: false, error: 'Conflicting membership' });
  });
  await connect(messages);
  const savedBefore = saves();
  await assert.rejects(messages.integrityRequest({ op: 'log_events_page' }));
  assert.equal(closed(), false);
  assert.deepEqual(await messages.integrityRequest({ op: 'verify_log' }), { valid: false, error: 'Conflicting membership' });
  assert.equal(saves(), savedBefore);
});

test('a queued local integrity read survives the initial transport generation change', async (t) => {
  let reads = 0;
  const report = { valid: true, error: null };
  const { messages, closed, saves, states } = setup(t, undefined, async () => {
    reads++;
    return JSON.stringify(report);
  });
  // Queue before start has restored the account and announced connecting. The
  // transport generation changes while this local read waits behind restore.
  const starting = messages.start();
  const reading = messages.integrityRequest({ op: 'verify_log', space_id: 'sp_fixture' });
  await starting;
  assert.deepEqual(await reading, report);
  assert.equal(reads, 1);
  assert.equal(saves(), 0);
  assert.equal(closed(), false);
  assert.equal(states.some(state => state.kind === 'failed'), false);
});

test('an integrity read suspended in the core rejects its late response after session stop', async (t) => {
  let finish!: (value: string) => void;
  let entered!: () => void;
  const waiting = new Promise<string>(resolve => { finish = resolve; });
  const started = new Promise<void>(resolve => { entered = resolve; });
  const { messages, saves, states } = setup(t, undefined, async () => {
    entered();
    return waiting;
  });
  await messages.start();
  const reading = messages.integrityRequest({ op: 'verify_log', space_id: 'sp_fixture' });
  const refused = assert.rejects(reading, /sessão foi encerrada/);
  await started;
  messages.stop();
  const emittedBefore = states.length;
  finish(JSON.stringify({ valid: true, error: null }));
  await refused;
  assert.equal(states.length, emittedBefore);
  assert.equal(saves(), 0);
});
