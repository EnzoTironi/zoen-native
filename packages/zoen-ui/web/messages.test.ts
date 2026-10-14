import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages, retryAtMs, watchRetry, type Invite, type MessagesState, type RetryTimer } from './messages.ts';
import type { CoreOperation } from './worker-client.ts';
import type { LinkedAccount, StoredSession } from './session.ts';

const account: LinkedAccount = {
  identity: 'ab'.repeat(32), device: 'cd'.repeat(32), cert: 'cert', name: 'Fixture', handle: 'fixture',
  relay: 'http://127.0.0.1:9000', primary: 'ab'.repeat(32), primary_hpke: '', hpke_secret: '',
  link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0,
  profile_fields: '',
};

function update(retryAt: number | null): string {
  return JSON.stringify({ snapshot: 'snapshot', frames: [], conversations: [], retry_at_ms: retryAt });
}

test('one retry timer follows retry_at_ms and advances at that instant', () => {
  const scheduled: { at: number; fire: () => void; cleared: boolean }[] = [];
  const advances: number[] = [];
  let now = 1_000;
  const schedule = (delay: number, fire: () => void): RetryTimer => {
    const entry = { at: now + delay, fire, cleared: false };
    scheduled.push(entry);
    return { clear() { entry.cleared = true; } };
  };
  const advance = (at: number) => { advances.push(at); };
  let timer = watchRetry(null, retryAtMs(update(5_000)), () => now, schedule, advance);
  assert.equal(scheduled.length, 1);
  const first = scheduled[0];
  assert.ok(first);
  assert.equal(first.at, 5_000);
  assert.equal(first.cleared, false);

  now = 2_000;
  timer = watchRetry(timer, retryAtMs(update(9_000)), () => now, schedule, advance);
  assert.equal(first.cleared, true);
  const second = scheduled[1];
  assert.ok(second);
  assert.equal(second.at, 9_000);
  assert.equal(second.cleared, false);

  const cleared = watchRetry(timer, retryAtMs(update(null)), () => now, schedule, advance);
  assert.equal(cleared, null);
  assert.equal(second.cleared, true);
  assert.deepEqual(advances, []);

  now = 3_000;
  watchRetry(null, retryAtMs(update(4_500)), () => now, schedule, advance);
  const third = scheduled[2];
  assert.ok(third);
  assert.equal(third.at, 4_500);
  now = 4_500;
  third.fire();
  assert.deepEqual(advances, [4_500]);
});

test('a far retry waits in capped steps and advances only at that instant', () => {
  const ceiling = 2 ** 31 - 1;
  const scheduled: { delay: number; fire: () => void }[] = [];
  const advances: number[] = [];
  let now = 1_000;
  const retryAt = 1_000 + ceiling + 80_000;
  let scheduledCount = 0;
  const schedule = (delay: number, fire: () => void): RetryTimer => {
    scheduledCount += 1;
    if (scheduledCount > 4) throw new Error('retry timer is looping');
    scheduled.push({ delay, fire });
    if (delay > ceiling) fire();
    return { clear() {} };
  };
  const arm = () => {
    watchRetry(null, retryAt, () => now, schedule, (at) => {
      advances.push(at);
      if (at < retryAt) arm();
    });
  };
  arm();
  const first = scheduled[0];
  assert.ok(first);
  assert.equal(first.delay, ceiling);
  now = 1_001;
  first.fire();
  assert.deepEqual(advances, []);
  const second = scheduled[1];
  assert.ok(second);
  assert.equal(second.delay, ceiling);
  now = retryAt;
  second.fire();
  assert.deepEqual(advances, [retryAt]);
  assert.equal(scheduledCount, 2);
});

function liveCore(text: (operation: CoreOperation) => Promise<string>) {
  let terminated = false;
  return {
    terminated: () => terminated,
    core: {
      text: async (operation: CoreOperation) => {
        if (terminated) throw new Error('The browser core is closed.');
        return text(operation);
      },
      bytes: async (operation: CoreOperation) => new TextEncoder().encode(operation.kind),
      close() { terminated = true; },
    },
  };
}

test('an out of date browser session clears the linked snapshot and returns to pairing', async () => {
  const saved: StoredSession[] = [];
  const states: string[] = [];
  let unpaired = 0;
  const { core } = liveCore(async (operation) => {
    if (operation.kind === 'restore') throw new Error('This browser session is out of date. Pair this browser again.');
    if (operation.kind === 'create_link') return 'zoen-link:ready';
    throw new Error(operation.kind);
  });
  const messages = new Messages({
    core,
    vault: { save: async (session) => { saved.push(session); }, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '{"version":1}' },
    changed: (state) => { states.push(state.kind); },
    unpaired: () => { unpaired += 1; },
  });
  await messages.start();
  assert.deepEqual(saved, [{ kind: 'empty' }]);
  assert.equal(unpaired, 1);
  assert.deepEqual(states, []);
  assert.equal(await core.text({ kind: 'create_link', device: account.device, relay: account.relay }), 'zoen-link:ready');
});

test('a different restore failure keeps the linked snapshot', async () => {
  const saved: StoredSession[] = [];
  let unpaired = 0;
  let failed = '';
  const { core } = liveCore(async () => { throw new Error('storage failed'); });
  const messages = new Messages({
    core,
    vault: { save: async (session) => { saved.push(session); }, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '{"version":2}' },
    changed: (state) => { if (state.kind === 'failed') failed = state.message; },
    unpaired: () => { unpaired += 1; },
  });
  await assert.rejects(messages.start(), /storage failed/);
  assert.deepEqual(saved, []);
  assert.equal(unpaired, 0);
  assert.equal(failed, 'storage failed');
  await assert.rejects(core.text({ kind: 'create_link', device: account.device, relay: account.relay }), /The browser core is closed/);
});

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

function installSocket(t: test.TestContext) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Socket.instances.length = 0;
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  t.mock.timers.enable({ apis: ['setTimeout'] });
  t.after(() => { if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
}

async function settle() { await new Promise<void>((resolve) => setImmediate(resolve)); }

async function authenticate() {
  const current = Socket.instances.at(-1);
  assert.ok(current);
  current.open(); await settle();
  current.frame({ kind: 'challenge', nonce: 'nonce', relay: '127.0.0.1:9000', protocol: 4 }); await settle();
  current.frame({ kind: 'ready', identity: account.identity, registered: true }); await settle();
}

const invite: Invite = { space: 'sp_join', title: 'Kept', role: 'Member', members: 2 };

async function secondJoinShows(t: test.TestContext, language: string, line: string) {
  installSocket(t);
  const states: MessagesState[] = [];
  let joins = 0;
  const { core, terminated } = liveCore(async (operation) => {
    if (operation.kind === 'restore') return '[]';
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === 'bootstrap' || operation.kind === 'receive') return update(null);
    if (operation.kind === 'queue_join') {
      if (operation.code === 'ZZZZZZZZZZ') throw new Error('Invalid invitation.');
      joins += 1;
      if (joins > 1) throw new Error('This invitation is already pending.');
      return update(null);
    }
    throw new Error(operation.kind);
  });
  const messages = new Messages({
    core, language,
    vault: { save: async () => {}, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '' },
    changed: (state) => { states.push(state); },
    unpaired: () => { throw new Error('pairing'); },
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  await messages.join({ invite, code: 'ABC2345678' });
  await assert.rejects(messages.join({ invite, code: 'ABC2345678' }), (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.equal(error.message, line);
    return true;
  });
  const invalid = language.toLowerCase().startsWith('pt') ? 'Este convite não é válido.' : 'Invalid invitation.';
  await assert.rejects(messages.join({ invite, code: 'ZZZZZZZZZZ' }), (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.equal(error.message, invalid);
    return true;
  });
  assert.equal(states.at(-1)?.kind, 'active');
  assert.equal(terminated(), false);
  assert.equal(joins, 2);
}

test('a second join in Portuguese keeps the linked session', async (t) => {
  await secondJoinShows(t, 'pt-BR', 'Este convite já está a caminho.');
});

test('a second join in English keeps the linked session', async (t) => {
  await secondJoinShows(t, 'en', 'This invitation is already on its way.');
});

const sendingConversation = {
  id: 'sp_join', title: 'Kept', kind: 'group', can_send: true, waiting: null, unreadable: 0, messages: [], removed: false,
};

async function refusedActionsShow(t: test.TestContext, language: string, lines: { invite: string; send: string; remove: string }) {
  installSocket(t);
  const states: MessagesState[] = [];
  const { core, terminated } = liveCore(async (operation) => {
    if (operation.kind === 'restore') return JSON.stringify([sendingConversation]);
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === 'bootstrap' || operation.kind === 'receive') {
      return JSON.stringify({ snapshot: 'snapshot', frames: [], conversations: [sendingConversation], retry_at_ms: null });
    }
    if (operation.kind === 'queue_join') throw new Error('Invalid invitation.');
    if (operation.kind === 'queue_message') throw new Error('Write a message of up to 32 KB.');
    if (operation.kind === 'delete_conversation') throw new Error('This conversation is still active.');
    throw new Error(operation.kind);
  });
  const messages = new Messages({
    core, language,
    vault: { save: async () => {}, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '' },
    changed: (state) => { states.push(state); },
    unpaired: () => { throw new Error('pairing'); },
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  await assert.rejects(messages.join({ invite, code: 'ZZZZZZZZZZ' }), (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.equal(error.message, lines.invite);
    return true;
  });
  await assert.rejects(messages.send({ space: 'sp_join', text: 'x'.repeat(32_769) }), (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.equal(error.message, lines.send);
    return true;
  });
  await assert.rejects(messages.deleteConversation('sp_join'), (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.equal(error.message, lines.remove);
    return true;
  });
  assert.equal(states.at(-1)?.kind, 'active');
  assert.equal(terminated(), false);
}

test('a Portuguese session shows refusals in Portuguese', async (t) => {
  await refusedActionsShow(t, 'pt-BR', {
    invite: 'Este convite não é válido.',
    send: 'Escreva uma mensagem de até 32 KB.',
    remove: 'Esta conversa ainda está ativa.',
  });
});

test('an English session shows the core refusal sentences', async (t) => {
  await refusedActionsShow(t, 'en', {
    invite: 'Invalid invitation.',
    send: 'Write a message of up to 32 KB.',
    remove: 'This conversation is still active.',
  });
});

test('forgetting this browser while a message is sending never writes the conversation back', async (t) => {
  installSocket(t);
  const states: MessagesState[] = [];
  const saved: StoredSession[] = [];
  let release: (value: string) => void = () => {};
  const { core } = liveCore(async (operation) => {
    if (operation.kind === 'restore') return JSON.stringify([sendingConversation]);
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === 'bootstrap' || operation.kind === 'receive') {
      return JSON.stringify({ snapshot: 'before', frames: [], conversations: [sendingConversation], retry_at_ms: null });
    }
    if (operation.kind === 'queue_message') {
      return new Promise<string>((resolve) => { release = resolve; });
    }
    throw new Error(operation.kind);
  });
  const messages = new Messages({
    core, language: 'en',
    vault: { save: async (session) => { saved.push(session); }, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '' },
    changed: (state) => { states.push(state); },
    unpaired: () => { throw new Error('pairing'); },
  });
  await messages.start();
  await authenticate();
  const before = saved.length;
  const sending = messages.send({ space: 'sp_join', text: 'hello' });
  await settle();
  messages.stop();
  core.close();
  release(JSON.stringify({ snapshot: 'after', frames: [], conversations: [sendingConversation], retry_at_ms: Date.now() + 1000 }));
  await sending.catch(() => {});
  t.mock.timers.tick(5_000);
  await settle();
  assert.equal(saved.length, before);
  assert.ok(states.every((state) => state.kind !== 'failed'));
});

type MidAction = 'send' | 'receive' | 'join' | 'delete';

async function forgetDuring(t: test.TestContext, action: MidAction) {
  installSocket(t);
  const states: MessagesState[] = [];
  const saved: StoredSession[] = [];
  let ready = false;
  let pending: (value: string) => void = () => {};
  const late = JSON.stringify({ snapshot: 'late', frames: [], conversations: [sendingConversation], retry_at_ms: Date.now() + 1000 });
  const hold = () => new Promise<string>((resolve) => { pending = resolve; ready = true; });
  const operationFor = { send: 'queue_message', receive: 'receive', join: 'queue_join', delete: 'delete_conversation' }[action];
  let bootstrapped = false;
  const { core } = liveCore(async (operation) => {
    if (operation.kind === 'restore') return JSON.stringify([sendingConversation]);
    if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
    if (operation.kind === operationFor && (operation.kind !== 'receive' || bootstrapped)) return hold();
    if (operation.kind === 'bootstrap' || operation.kind === 'receive') {
      bootstrapped = true;
      return JSON.stringify({ snapshot: 'before', frames: [], conversations: [sendingConversation], retry_at_ms: null });
    }
    throw new Error(operation.kind);
  });
  const messages = new Messages({
    core, language: 'pt-BR',
    vault: { save: async (session) => { saved.push(session); }, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '' },
    changed: (state) => { states.push(state); },
    unpaired: () => { throw new Error('pairing'); },
  });
  await messages.start();
  await authenticate();
  const before = saved.length;
  const running = action === 'send' ? messages.send({ space: 'sp_join', text: 'oi' }) :
    action === 'join' ? messages.join({ invite, code: 'ABC2345678' }) :
      action === 'delete' ? messages.deleteConversation('sp_join') :
        (Socket.instances.at(-1)?.frame({ kind: 'event' }), Promise.resolve());
  await settle(); await settle();
  assert.equal(ready, true, `${action} reached the core before the person forgot this browser`);
  messages.stop();
  core.close();
  pending(late);
  await running.catch(() => {});
  t.mock.timers.tick(5_000);
  await settle();
  assert.equal(saved.length, before, `${action} wrote the conversation back after forgetting`);
  assert.ok(states.every((state) => state.kind !== 'failed'), `${action} flashed a failure after forgetting`);
}

for (const action of ['send', 'receive', 'join', 'delete'] as const) {
  test(`forgetting this browser in the middle of ${action} leaves nothing behind`, async (t) => { await forgetDuring(t, action); });
}
