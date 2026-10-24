import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mutedFixture, mutedSnapshot, scope, deferred } from './muted-chats.fixtures.ts';
import { createMutedChatsPort } from '../notifications/muted-chats.ts';
import { ClientStore } from './store.ts';
import { Driver } from './store.fixtures.ts';

test('Core directory supplies only muted current-member chats, including archived chats', async () => {
  const f = mutedFixture();
  f.snapshot().conversations[0]!.archived = true;
  f.snapshot().conversations.push({
    ...f.snapshot().conversations[0]!,
    id: 'unmuted',
    muted: false,
  });
  f.snapshot().conversations.push({ ...f.snapshot().conversations[0]!, id: 'left', members: [] });
  await f.load();
  assert.deepEqual(
    f.session.getSnapshot().chats.map((chat) => chat.id),
    ['chat-0', 'chat-1'],
  );
  const ticket = f.session.ticket();
  assert.equal(f.session.canOpen('chat-0', ticket), false);
  f.select();
  assert.equal(f.session.canOpen('chat-0', ticket), true);
  await f.unmute();
  assert.deepEqual(f.writes(), [{ kind: 'write', id: 'chat-0', owner: scope }]);
  assert.deepEqual(
    f.session.getSnapshot().chats.map((chat) => chat.id),
    ['chat-1'],
  );
  assert.equal(f.session.getSnapshot().pending, null);
  assert.equal(f.session.canOpen('chat-0', ticket), false);
});

test('cancel, duplicate taps and unselected identities never dispatch another mutation', async () => {
  const f = mutedFixture();
  await f.load();
  f.select();
  const answer = deferred<boolean>();
  let prompts = 0;
  const work = f.unmute(() => {
    ++prompts;
    return answer.promise;
  });
  await f.unmute(async () => {
    ++prompts;
    return true;
  });
  await f.unmute(async () => true, 'chat-1');
  answer.resolve(false);
  await work;
  assert.equal(prompts, 1);
  assert.equal(f.writes().length, 0);
  assert.equal(f.session.getSnapshot().pending, null);
});
for (const [field, value] of [
  ['identity', 'other'],
  ['device', 'other'],
  ['relay', 'https://relay.example/'],
  ['epoch', 2],
] as const)
  test(`late confirmation is fenced by exact ${field}`, async () => {
    const f = mutedFixture();
    await f.load();
    f.select();
    const answer = deferred<boolean>();
    const work = f.unmute(() => answer.promise);
    f.scope({ ...scope, [field]: value });
    answer.resolve(true);
    await work;
    assert.equal(f.writes().length, 0);
    assert.equal(f.session.ticket(), null);
  });
test('blur/reopen and refreshed revision reject old dialogs without clearing newer selection', async () => {
  const f = mutedFixture();
  await f.load();
  f.select();
  const old = deferred<boolean>();
  const pending = f.unmute(() => old.promise);
  f.session.retire();
  f.session.activate(scope);
  await f.load();
  f.select('chat-1');
  const next = deferred<boolean>();
  const fresh = f.unmute(() => next.promise, 'chat-1');
  old.resolve(true);
  await pending;
  assert.equal(f.session.getSnapshot().pending, 'chat-1');
  next.resolve(false);
  await fresh;
  f.select();
  const review = deferred<boolean>();
  const work = f.unmute(() => review.promise);
  await f.load();
  review.resolve(true);
  await work;
  assert.equal(f.writes().length, 0);
});
for (const change of ['title', 'deadline', 'membership', 'unmuted'] as const)
  test(`reviewed target is reread before writing: changed ${change} cancels`, async () => {
    const f = mutedFixture();
    await f.load();
    f.select();
    await f.unmute(async () => {
      const chat = f.snapshot().conversations[0]!;
      if (change === 'title') chat.title = 'Replaced title';
      if (change === 'deadline') chat.muted_until_ms = '9999999999999';
      if (change === 'membership') chat.members = [];
      if (change === 'unmuted') chat.muted = false;
      return true;
    });
    assert.equal(f.writes().length, 0);
    assert.equal(f.session.getSnapshot().phase, 'ready');
  });
test('pin/archive changes never enter the mute-only payload or reset another chat', async () => {
  const f = mutedFixture();
  await f.load();
  f.select();
  await f.unmute(async () => {
    f.snapshot().conversations[0]!.pinned = true;
    f.snapshot().conversations[0]!.archived = true;
    return true;
  });
  assert.equal(f.writes().length, 1);
  assert.equal(f.snapshot().conversations[0]!.pinned, true);
  assert.equal(f.snapshot().conversations[0]!.archived, true);
  assert.equal(f.snapshot().conversations[1]!.muted, true);
});
test('lost accepted response reconciles by read, without resend or remote-delivery claims', async () => {
  const f = mutedFixture();
  await f.load();
  f.select();
  f.write(async (id) => {
    const chat = f.snapshot().conversations.find((row) => row.id === id)!;
    chat.muted = false;
    chat.muted_until_ms = null;
    throw new Error('lost response after local commit');
  });
  await f.unmute();
  assert.equal(f.writes().length, 1);
  assert.equal(f.session.getSnapshot().actionFailed, false);
  assert.deepEqual(
    f.session.getSnapshot().chats.map((chat) => chat.id),
    ['chat-1'],
  );
  await f.load();
  assert.equal(f.writes().length, 1);
});
test('ambiguous write and failed read stay explicit; retry only reads', async () => {
  const f = mutedFixture();
  await f.load();
  f.select();
  f.write(async () => {
    f.read(async () => {
      throw new Error('unavailable');
    });
    throw new Error('unknown');
  });
  await f.unmute();
  assert.equal(f.writes().length, 1);
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.session.getSnapshot().actionFailed, true);
  assert.equal(f.session.getSnapshot().readFailed, true);
  f.read(async () => f.snapshot());
  await f.load();
  assert.equal(f.writes().length, 1);
  assert.equal(f.session.getSnapshot().phase, 'ready');
});
test('loading, empty, malformed and foreign reads stay separate; late reads are suppressed', async () => {
  const f = mutedFixture();
  const read = deferred<ReturnType<typeof mutedSnapshot>>();
  f.read(() => read.promise);
  const pending = f.load();
  assert.equal(f.session.getSnapshot().phase, 'loading');
  f.session.retire();
  read.resolve(f.snapshot());
  await pending;
  assert.equal(f.session.getSnapshot().phase, 'inactive');
  f.session.activate(scope);
  f.read(async () => ({
    ...f.snapshot(),
    account: { ...f.snapshot().account!, device_id: 'foreign' },
  }));
  await f.load();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  f.read(async () => ({
    ...f.snapshot(),
    conversations: [f.snapshot().conversations[0]!, f.snapshot().conversations[0]!],
  }));
  await f.load();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  f.read(async () => ({ ...f.snapshot(), conversations: [] }));
  await f.load();
  assert.equal(f.session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.session.getSnapshot().chats, []);
});
test('real ClientStore sends exact scoped mute-only command and retires stale ports', async () => {
  const snapshot = mutedSnapshot();
  const driver = new Driver();
  driver.opening = async () => snapshot;
  const calls: unknown[] = [];
  driver.operation = async (command) => {
    calls.push(command);
    if (command.op === 'snapshot') return snapshot;
    assert.deepEqual(command, {
      op: 'set_chat_muted',
      space_id: 'chat-0',
      muted: false,
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
    });
    snapshot.conversations[0]!.muted = false;
    snapshot.conversations[0]!.muted_until_ms = null;
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const port = createMutedChatsPort(store);
  const owner = port.scope()!;
  assert.equal((await port.read()).conversations[0]!.muted, true);
  await port.unmute('chat-0', owner);
  assert.equal(store.getSnapshot().snapshot!.conversations[0]!.muted, false);
  await store.close();
  const count = calls.length;
  await assert.rejects(port.unmute('chat-0', owner));
  assert.equal(calls.length, count);
});

for (const refresh of ['canonical', 'foreground'] as const)
  for (const result of ['accepted', 'unknown'] as const)
    test(`${refresh} refresh preserves a dispatched-write fence until ${result} ACK reconciliation`, async () => {
      const f = mutedFixture();
      await f.load();
      f.select();
      const started = deferred<void>();
      const ack = deferred<void>();
      f.write(async (id) => {
        started.resolve();
        await ack.promise;
        if (result === 'unknown') throw new Error('uncertain acceptance');
        const chat = f.snapshot().conversations.find((row) => row.id === id)!;
        chat.muted = false;
        chat.muted_until_ms = null;
      });
      const first = f.unmute();
      await started.promise;
      f.snapshot().conversations[1]!.title = 'Other chat changed';
      if (refresh === 'foreground') {
        f.session.retire();
        f.session.activate(scope);
      }
      await f.load();
      assert.equal(f.session.getSnapshot().pending, 'chat-0');
      f.select();
      const second = f.unmute();
      await new Promise<void>((resolve) => setImmediate(resolve));
      try {
        assert.equal(f.writes().length, 1, 'reads must not release the in-flight write');
      } finally {
        ack.resolve();
        await Promise.all([first, second]);
      }
      assert.equal(f.session.getSnapshot().pending, null);
      assert.equal(f.session.getSnapshot().phase, 'ready');
      assert.equal(f.session.getSnapshot().actionFailed, result === 'unknown');
      assert.equal(
        f.session.getSnapshot().chats.some((chat) => chat.id === 'chat-0'),
        result === 'unknown',
      );
      assert.equal(f.writes().length, 1, 'terminal reconciliation only reads');
    });
