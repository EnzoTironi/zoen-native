import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages, type MessagesState } from './messages.ts';
import type { CoreOperation } from './worker-client.ts';
import type { StoredSession } from './session.ts';
import { typingPeers, type CoreUiEvent } from './typing-state.ts';

// Unit boundary doubles only: this suite does not attest a live relay connection.
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
const identity = '11'.repeat(32), other = '22'.repeat(32);
const conversation = { id: 'sp_chat', title: 'Chat', kind: 'group', can_send: true, waiting: null,
  unreadable: 0, unread: 0, mentions: 0, members: 2, messages: [] };
const session: Extract<StoredSession, { kind: 'linked' }> = { kind: 'linked', snapshot: '', account: {
  identity, device: '33'.repeat(32), cert: 'unit certificate', name: 'Me', handle: 'me', relay: 'https://relay.example', primary: identity,
  primary_hpke: '', hpke_secret: '', link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0, profile_fields: '',
} };
const settle = () => new Promise<void>(resolve => setImmediate(resolve));
function setup(t: test.TestContext) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  const states: MessagesState[] = [];
  let events: unknown[] = [], receives = 0;
  let gate: Promise<void> | null = null, release: (() => void) | undefined;
  const snapshot = () => JSON.stringify({ snapshot: 'unit snapshot', frames: [], conversations: [conversation], ui_events: events });
  const core = { close() {}, bytes: async () => new Uint8Array([1]), text: async (operation: CoreOperation) => {
    if (operation.kind === 'restore') return JSON.stringify([conversation]);
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === 'bootstrap') return snapshot();
    if (operation.kind === 'receive') { receives++; return snapshot(); }
    throw new Error('Unexpected unit operation');
  } };
  const messages = new Messages({ core, session, changed: state => states.push(state),
    vault: { sign: async () => new Uint8Array(64), save: async () => { if (gate) await gate; } } });
  t.after(() => { messages.stop(); if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
  return { messages, states, receives: () => receives,
    async emit(next: unknown[]) { events = next; Socket.current.frame({ kind: 'other' }); await settle(); },
    holdSave() { gate = new Promise<void>(resolve => { release = resolve; }); }, releaseSave() { release?.(); gate = null; } };
}
async function connect(messages: Messages) {
  await messages.start();
  Socket.current.readyState = Socket.OPEN; Socket.current.onopen?.(); await settle();
  Socket.current.frame({ kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 }); await settle();
  Socket.current.frame({ kind: 'ready', identity, registered: true }); await settle();
}
function peers(states: MessagesState[]) {
  const state = states.at(-1);
  return state?.kind === 'active' ? typingPeers(state.typing ?? {}, 'sp_chat') : [];
}
const active: CoreUiEvent = { kind: 'typing', space: 'sp_chat', from: other, name: 'Ana' };

test('authenticated receive applies Core typing events, renewal expiry and stopped through normal updates', async t => {
  t.mock.timers.enable({ apis: ['Date', 'setTimeout'], now: 1_000 });
  const runtime = setup(t); await connect(runtime.messages);
  await runtime.emit([active]);
  assert.equal(runtime.receives(), 1); assert.equal(peers(runtime.states)[0]?.name, 'Ana');
  t.mock.timers.tick(3_000); await runtime.emit([active]);
  t.mock.timers.tick(3_000); assert.equal(peers(runtime.states).length, 1);
  t.mock.timers.tick(3_000); assert.equal(peers(runtime.states).length, 0);
  await runtime.emit([active]); await runtime.emit([{ kind: 'stopped', space: 'sp_chat', from: other }]);
  assert.equal(peers(runtime.states).length, 0);
});

test('reconnect clears typing and an update awaiting persistence cannot revive stale-generation state', async t => {
  const runtime = setup(t); await connect(runtime.messages);
  await runtime.emit([active]); assert.equal(peers(runtime.states).length, 1);
  runtime.holdSave(); await runtime.emit([active]);
  Socket.current.onclose?.({ code: 1006 });
  assert.equal(peers(runtime.states).length, 0);
  runtime.releaseSave(); await settle();
  assert.equal(peers(runtime.states).length, 0);
});

test('malformed Core UI events fail closed instead of publishing arbitrary typing', async t => {
  const runtime = setup(t); await connect(runtime.messages);
  await runtime.emit([{ ...active, from: 'invalid' }]);
  assert.equal(runtime.states.at(-1)?.kind, 'failed');
  assert.equal(peers(runtime.states).length, 0);
});
