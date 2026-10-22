import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages, type MessagesState } from './messages.ts';
import type { CoreOperation } from './worker-client.ts';
import {
  install,
  settle,
  authenticate,
  socket,
  Socket,
  linked,
  conversation,
  vault,
  messagingCore,
} from './transport-tests/fixture.ts';

const policy = JSON.stringify({
  version: 1,
  interval_ms: 60_000,
  timeout_ms: 30_000,
  initial_inactivity_ms: 60_000,
});
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function fixture(t: test.TestContext, save: (snapshot: string) => Promise<void> = async () => {}) {
  install(t);
  let now = 0,
    bootstraps = 0,
    admission: number | null = null,
    deadline = 60_000;
  t.mock.method(performance, 'now', () => now);
  const calls: CoreOperation[] = [],
    states: MessagesState[] = [],
    saves: string[] = [];
  const base = messagingCore();
  const update = (
    snapshot: string,
    frames: number[][],
    admission: number | null,
    deadline: number,
  ) =>
    JSON.stringify({
      snapshot,
      frames,
      conversations: [{ ...conversation, title: snapshot }],
      cursor_repair: { admission_ms: admission, abort_deadline_ms: deadline },
    });
  const core = {
    ...base,
    get closed() {
      return base.closed;
    },
    text: async (op: CoreOperation): Promise<string> => {
      calls.push(op);
      if (op.kind === 'cursor_repair_config') return policy;
      if (op.kind === 'bootstrap') {
        admission = null;
        deadline = op.repairNow + 60_000;
        return update(`bootstrap${++bootstraps}`, [[7]], admission, deadline);
      }
      if (op.kind === 'receive') {
        const frame = JSON.parse(new TextDecoder().decode(op.bytes));
        if (frame.kind === 'sync_done') {
          admission = op.repairNow + 60_000;
          deadline = op.repairNow + 90_000;
          return update('caught-up', [], admission, deadline);
        }
        return update('received', [], admission, deadline);
      }
      if (op.kind === 'reconcile') {
        admission = null;
        return JSON.stringify({
          kind: 'update',
          update: JSON.parse(update('full-sync', [[90]], admission, deadline)),
        });
      }
      if (op.kind === 'queue_message') return update('retained-write', [[8]], admission, deadline);
      return base.text(op);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (value) => {
        saves.push(value.snapshot);
        await save(value.snapshot);
      },
    },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  const advance = async (value: number) => {
    now += value;
    t.mock.timers.tick(value);
    await settle();
  };
  return {
    messages,
    core,
    calls,
    states,
    saves,
    advance,
    set: (value: number) => {
      now = value;
    },
    current: () => states.at(-1),
    receive: async (frame: unknown) => {
      socket().frame(frame);
      await settle();
    },
  };
}
function title(state: MessagesState | undefined) {
  return state?.kind === 'active' ? state.conversations[0]?.title : undefined;
}

test('the production browser controller persists a periodic full Sync despite a healthy Pong stream', async (t) => {
  const held = deferred();
  const f = fixture(t, async (snapshot) => {
    if (snapshot === 'full-sync') await held.promise;
  });
  await f.messages.start();
  await authenticate();
  await f.receive({ kind: 'sync_done' });
  const original = socket();
  await f.advance(30_000);
  await f.receive({ kind: 'pong' });
  await f.advance(30_000);
  assert.equal(f.calls.filter((op) => op.kind === 'reconcile').length, 1);
  assert.equal(f.saves.at(-1), 'full-sync');
  assert.equal(
    original.sent.some((bytes) => bytes[0] === 90),
    false,
    'Sync must wait for durable save',
  );
  held.resolve();
  await settle();
  assert.equal(original.sent.filter((bytes) => bytes[0] === 90).length, 1);
  assert.equal(title(f.current()), 'full-sync');
  await f.receive({ kind: 'pong' });
  await f.advance(29_999);
  assert.equal(f.current()?.kind, 'active');
  await f.advance(1);
  const state = f.current();
  assert.equal(state?.kind === 'active' && state.connection.kind, 'reconnecting');
  assert.equal(
    f.core.closed,
    false,
    'A missing SyncDone retires the socket, not the encrypted account',
  );
});

test('blocked bootstrap persistence cannot starve the watchdog or send/project into its replacement', async (t) => {
  const held = deferred();
  const f = fixture(t, async (snapshot) => {
    if (snapshot === 'bootstrap1') await held.promise;
  });
  await f.messages.start();
  await authenticate();
  const old = socket();
  let closeCalls = 0;
  old.close = () => {
    ++closeCalls;
  }; // Browser close event is intentionally delayed.
  await f.advance(30_000);
  await f.receive({ kind: 'pong' });
  await f.advance(30_000);
  const state = f.current();
  assert.equal(state?.kind === 'active' && state.connection.kind, 'reconnecting');
  assert.equal(closeCalls, 1);
  assert.equal(old.readyState, Socket.OPEN);
  assert.equal(
    old.sent.some((bytes) => bytes[0] === 7),
    false,
  );
  await f.advance(1_000);
  await authenticate();
  const replacement = socket();
  assert.notEqual(replacement, old);
  held.resolve();
  await settle();
  await settle();
  assert.equal(title(f.current()), 'bootstrap2');
  assert.equal(replacement.sent.filter((bytes) => bytes[0] === 7).length, 1);
  assert.equal(
    old.sent.some((bytes) => bytes[0] === 7),
    false,
  );
  old.onclose?.({ code: 1006 });
  await settle();
  assert.equal(f.current()?.kind === 'active' && f.current()?.connection.kind, 'ready');
  await f.advance(30_000);
  await f.receive({ kind: 'pong' });
  await f.advance(28_999);
  assert.equal(f.current()?.kind === 'active' && f.current()?.connection.kind, 'ready');
  assert.equal(f.core.closed, false);
});

test('a write held beyond the absolute deadline retires before its late frame or metadata can escape', async (t) => {
  const held = deferred();
  const f = fixture(t, async (snapshot) => {
    if (snapshot === 'retained-write') await held.promise;
  });
  await f.messages.start();
  await authenticate();
  await f.receive({ kind: 'sync_done' });
  const old = socket();
  old.close = () => {};
  const sent = f.messages.send({ space: conversation.id, text: 'synthetic retained text' });
  await settle();
  // The Promise resolves before overdue timer callbacks get an event-loop turn.
  f.set(90_000);
  held.resolve();
  await sent;
  await settle();
  assert.equal(
    old.sent.some((bytes) => bytes[0] === 8),
    false,
  );
  assert.notEqual(title(f.current()), 'retained-write');
  assert.equal(f.current()?.kind === 'active' && f.current()?.connection.kind, 'reconnecting');
  assert.equal(
    f.calls.some((op) => op.kind === 'reconcile'),
    false,
  );
  assert.equal(f.core.closed, false);
});

test('a queued periodic scan never executes after blocked work exhausts its budget', async (t) => {
  const held = deferred();
  const f = fixture(t, async (snapshot) => {
    if (snapshot === 'retained-write') await held.promise;
  });
  await f.messages.start();
  await authenticate();
  await f.receive({ kind: 'sync_done' });
  const old = socket();
  old.close = () => {};
  const sent = f.messages.send({ space: conversation.id, text: 'synthetic' });
  await settle();
  await f.advance(30_000);
  await f.receive({ kind: 'pong' });
  await f.advance(30_000); // Scan is queued behind the saved write.
  assert.equal(
    f.calls.some((op) => op.kind === 'reconcile'),
    false,
  );
  await f.advance(30_000);
  held.resolve();
  await sent;
  await settle();
  assert.equal(
    f.calls.some((op) => op.kind === 'reconcile'),
    false,
  );
  assert.equal(
    old.sent.some((bytes) => bytes[0] === 8),
    false,
  );
  assert.equal(f.current()?.kind === 'active' && f.current()?.connection.kind, 'reconnecting');
});

test('unsupported/missing Core policy is refused before any browser socket starts', async (t) => {
  install(t);
  for (const value of [
    '{}',
    '{"version":0}',
    '{"version":1,"interval_ms":0,"timeout_ms":30000,"initial_inactivity_ms":60000}',
  ]) {
    const base = messagingCore();
    const core = {
      ...base,
      text: async (op: CoreOperation) =>
        op.kind === 'cursor_repair_config' ? value : base.text(op),
    };
    const messages = new Messages({
      core,
      session: linked,
      vault: { ...vault, save: async () => {} },
      changed: () => {},
    });
    await assert.rejects(messages.start());
    assert.equal(Socket.instances.length, 0);
    assert.equal(base.closed, true);
    messages.stop();
  }
});
