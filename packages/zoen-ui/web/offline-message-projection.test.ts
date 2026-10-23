import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages, type MessagesState } from './messages.ts';
import type { CoreOperation } from './worker-client.ts';
import {
  install,
  settle,
  authenticate,
  socket,
  linked,
  conversation,
  vault,
  messagingCore,
} from './transport-tests/fixture.ts';

function gate() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const queued = {
  id: 'local-message',
  entry: 'local-message',
  author: linked.account.identity,
  name: linked.account.name,
  text: 'Keep this offline message',
  at_ms: 1,
  delivery: 'sending',
  failure: null,
  reactions: [],
};
const retained = JSON.stringify({ outbox: ['encrypted local event'], messages: [queued] });

// Only the external Core/vault/socket ports are controlled. Queue admission,
// connection generations, durable-before-emit and projection use real Messages.
function fixture(
  t: test.TestContext,
  hold: 'core' | 'save',
  rejectSave = false,
  holdBootstrap = false,
) {
  install(t);
  const entered = gate(),
    release = gate();
  const bootstrapEntered = gate(),
    releaseBootstrap = gate();
  let bootstraps = 0,
    localQueued = false;
  const states: MessagesState[] = [],
    saves: string[] = [],
    calls: CoreOperation[] = [];
  const base = messagingCore();
  const update = (snapshot: string, messages: unknown[]) =>
    JSON.stringify({
      snapshot,
      frames: [[8]],
      conversations: [{ ...conversation, messages }],
    });
  const core = {
    ...base,
    text: async (op: CoreOperation): Promise<string> => {
      calls.push(op);
      if (op.kind === 'bootstrap' && holdBootstrap) {
        if (++bootstraps === 2) {
          bootstrapEntered.resolve();
          await releaseBootstrap.promise;
        }
        return JSON.stringify({
          snapshot: 'fresh bootstrap',
          frames: [],
          conversations: [{ ...conversation, messages: localQueued ? [queued] : [] }],
        });
      }
      if (op.kind === 'queue_post') {
        assert.equal(op.text, queued.text);
        if (hold === 'core') {
          entered.resolve();
          await release.promise;
        }
        localQueued = true;
        return update(retained, [queued]);
      }
      if (op.kind === 'receive') {
        entered.resolve();
        await release.promise;
        return update('remote snapshot', [{ ...queued, id: 'remote-message' }]);
      }
      return base.text(op);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (value) => {
        if (value.snapshot === retained) {
          if (hold === 'save') {
            entered.resolve();
            await release.promise;
          }
          if (rejectSave) throw new Error('Synthetic vault write refused');
        }
        saves.push(value.snapshot);
      },
    },
    changed: (state) => states.push(state),
  });
  t.after(() => {
    release.resolve();
    releaseBootstrap.resolve();
    messages.stop();
  });
  return { messages, states, saves, calls, entered, release, bootstrapEntered, releaseBootstrap };
}

function visible(states: MessagesState[]) {
  const state = states.at(-1);
  return state?.kind === 'active' ? state.conversations[0]?.messages : [];
}

for (const hold of ['core', 'save'] as const) {
  test(`offline post stays visible when transport reconnects during ${hold}`, async (t) => {
    const f = fixture(t, hold);
    await f.messages.start();
    const original = socket();
    const sent = f.messages.post({ space: conversation.id, text: queued.text });
    await f.entered.promise;
    assert.deepEqual(visible(f.states), [], 'uncommitted work is not projected');
    original.close();
    t.mock.timers.tick(1_000);
    const replacement = socket();
    assert.notEqual(replacement, original);
    f.release.resolve();
    await sent;
    assert.deepEqual(f.saves, [retained]);
    assert.equal(f.calls.filter((op) => op.kind === 'queue_post').length, 1);
    assert.equal(visible(f.states)?.length, 1);
    assert.equal(visible(f.states)?.[0]?.id, queued.id);
    assert.equal(visible(f.states)?.[0]?.text, queued.text);
    assert.equal(visible(f.states)?.[0]?.delivery, 'sending');
    assert.deepEqual(original.sent, []);
    assert.deepEqual(replacement.sent, [], 'a retired frame cannot enter the new socket');
    assert.equal(
      f.states.at(-1)?.kind === 'active' && f.states.at(-1)?.connection.kind,
      'reconnecting',
    );
  });
}

test('Ready2 during save1 retains the local post before its queued bootstrap completes', async (t) => {
  const f = fixture(t, 'save', false, true);
  await f.messages.start();
  await authenticate();
  const original = socket();
  const sent = f.messages.post({ space: conversation.id, text: queued.text });
  await f.entered.promise;
  original.close();
  t.mock.timers.tick(1_000);
  await authenticate();
  const replacement = socket();
  assert.notEqual(replacement, original);
  f.release.resolve();
  await sent;
  await f.bootstrapEntered.promise;
  assert.equal(f.saves.filter((snapshot) => snapshot === retained).length, 1);
  assert.equal(visible(f.states)?.[0]?.id, queued.id);
  assert.equal(original.sent.filter((bytes) => bytes[0] === 8).length, 0);
  assert.equal(replacement.sent.filter((bytes) => bytes[0] === 8).length, 0);
  f.releaseBootstrap.resolve();
  await settle();
  assert.equal(visible(f.states)?.length, 1);
  assert.equal(visible(f.states)?.[0]?.id, queued.id);
  assert.equal(f.calls.filter((op) => op.kind === 'queue_post').length, 1);
});

test('an online local post emits once only after its snapshot is durable', async (t) => {
  const f = fixture(t, 'save');
  await f.messages.start();
  await authenticate();
  const original = socket();
  const sent = f.messages.post({ space: conversation.id, text: queued.text });
  await f.entered.promise;
  assert.equal(original.sent.filter((bytes) => bytes[0] === 8).length, 0);
  assert.deepEqual(visible(f.states), []);
  f.release.resolve();
  await sent;
  assert.equal(original.sent.filter((bytes) => bytes[0] === 8).length, 1);
  assert.equal(visible(f.states)?.[0]?.id, queued.id);
  assert.equal(f.saves.filter((value) => value === retained).length, 1);
});

test('a refused vault write cannot publish a local post or resolve it as accepted', async (t) => {
  const f = fixture(t, 'save', true);
  await f.messages.start();
  const sent = f.messages.post({ space: conversation.id, text: queued.text });
  const refused = assert.rejects(sent, /Synthetic vault write refused/);
  await f.entered.promise;
  f.release.resolve();
  await refused;
  assert.deepEqual(f.saves, []);
  assert.equal(
    f.states.some((s) => s.conversations[0]?.messages.length),
    false,
  );
  assert.deepEqual(socket().sent, []);
});

test('closing the owner while its local save is held cannot publish into either owner', async (t) => {
  const f = fixture(t, 'save');
  await f.messages.start();
  const sent = f.messages.post({ space: conversation.id, text: queued.text });
  await f.entered.promise;
  const original = socket();
  f.messages.stop();
  const count = f.states.length;
  const next: MessagesState[] = [];
  const replacement = new Messages({
    core: messagingCore(),
    session: {
      ...linked,
      account: { ...linked.account, identity: '44'.repeat(32), device: '55'.repeat(32) },
    },
    vault: { ...vault, save: async () => assert.fail('old owner cannot write the new vault') },
    changed: (state) => next.push(state),
  });
  t.after(() => replacement.stop());
  await replacement.start();
  f.release.resolve();
  await sent;
  assert.deepEqual(f.saves, [retained]);
  assert.equal(f.states.length, count);
  assert.deepEqual(visible(next), []);
  assert.deepEqual(original.sent, []);
  assert.deepEqual(socket().sent, []);
});

test('remote ingestion still cannot project into a retired transport generation', async (t) => {
  const f = fixture(t, 'core');
  await f.messages.start();
  await authenticate();
  const original = socket();
  original.frame({ kind: 'event' });
  await f.entered.promise;
  original.close();
  t.mock.timers.tick(1_000);
  const replacement = socket();
  f.release.resolve();
  await settle();
  await settle();
  assert.ok(f.saves.includes('remote snapshot'));
  assert.deepEqual(visible(f.states), []);
  assert.equal(replacement.sent.filter((bytes) => bytes[0] === 8).length, 0);
});
