import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  account,
  author,
  deferred,
  fixture,
  invalidSessions,
  message,
  spaceId,
} from './message-author-navigation.fixtures.ts';
import type { TimelineEntry } from './types';

test('author profile rereads canonical human and agent identities without changing messages', async () => {
  for (const kind of ['Person', 'Agent'] as const) {
    const f = fixture();
    f.read(async () => message({ author: author(`canonical_${kind}`, kind) }));
    await f.actions.profile(message({ author: author('stale_recycled_author') }));
    assert.deepEqual(f.reads, [
      { op: 'message', space_id: spaceId, message_id: 'synthetic_message' },
    ]);
    assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: `canonical_${kind}` } }]);
    assert.deepEqual(f.mutations, []);
    assert.deepEqual(f.alerts, []);
  }
});

test('a duplicate author press shares one read and accepted navigation stays latched until focus changes', async () => {
  const f = fixture();
  const read = deferred<TimelineEntry>();
  f.read(() => read.promise);
  const first = f.actions.profile(message());
  await f.actions.profile(message());
  assert.equal(f.reads.length, 1);
  read.resolve(message());
  await first;
  await f.render().profile(message());
  assert.equal(f.reads.length, 1);
  assert.equal(f.routes.length, 1);
  f.blur();
  f.focus();
  await f.render().profile(message());
  assert.equal(f.reads.length, 2);
  assert.equal(f.routes.length, 2);
});

test('saved author callbacks recheck live session and membership before any canonical read', async (t) => {
  for (const [name, change] of invalidSessions) {
    await t.test(name, async () => {
      const f = fixture();
      f.update(change);
      await f.actions.profile(message());
      assert.deepEqual(f.reads, []);
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.alerts, []);
    });
  }
});

test('disabled, missing, deleted and non-message canonical rows reject saved author callbacks', async () => {
  for (const patch of [
    { disabled: true },
    { entries: [] },
    { entries: [message({ deleted: true })] },
    { entries: [message({ kind: { System: { text: 'Synthetic system event' } } })] },
  ]) {
    const f = fixture();
    f.render(patch);
    await f.actions.profile(message());
    assert.deepEqual(f.reads, []);
    assert.deepEqual(f.routes, []);
  }
});

test('callbacks captured for another rendered owner, device, mode or source chat remain detached', async () => {
  for (const change of [
    account({ identity_id: 'bob' }),
    account({ device_id: 'new_phone' }),
    invalidSessions[2]![1],
  ]) {
    const f = fixture();
    f.update(change);
    f.render();
    await f.actions.profile(message());
    assert.deepEqual(f.reads, []);
  }
  const f = fixture();
  f.update((state) => ({
    ...state,
    snapshot: {
      ...state.snapshot!,
      conversations: [
        ...state.snapshot!.conversations,
        { ...state.snapshot!.conversations[0]!, id: 'other_chat' },
      ],
    },
  }));
  f.render({ spaceId: 'other_chat' });
  await f.actions.profile(message());
  assert.deepEqual(f.reads, []);
});

test('blur and unmount reject saved callbacks and suppress pending-read UI work', async () => {
  for (const lifecycle of ['blur', 'unmount'] as const) {
    const f = fixture();
    const read = deferred<TimelineEntry>();
    f.read(() => read.promise);
    const pending = f.actions.profile(message());
    f[lifecycle]();
    const writes = f.writes.length;
    await f.actions.profile(message());
    assert.equal(f.reads.length, 1);
    read.resolve(message());
    await pending;
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.alerts, []);
    assert.equal(f.writes.length, writes);
  }
});

test('canonical read completion rechecks fresh session and source access without needing a render', async (t) => {
  for (const [name, change] of invalidSessions) {
    await t.test(name, async () => {
      const f = fixture();
      const read = deferred<TimelineEntry>();
      f.read(() => read.promise);
      const pending = f.actions.profile(message());
      f.update(change);
      const writes = f.writes.length;
      read.resolve(message());
      await pending;
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.alerts, []);
      assert.equal(f.writes.length, writes);
    });
  }
});

test('transient session or membership loss cannot revive a pending read after access returns', async (t) => {
  for (const [name, change] of invalidSessions) {
    await t.test(name, async () => {
      const f = fixture();
      const original = f.state();
      const oldRead = deferred<TimelineEntry>();
      const newRead = deferred<TimelineEntry>();
      f.read(() => oldRead.promise);
      const oldAttempt = f.actions.profile(message());
      f.update(change);
      f.update(() => original);
      f.read(() => newRead.promise);
      const newAttempt = f.actions.profile(message());
      assert.equal(f.reads.length, 2, 'A restored session can start a fresh attempt');
      oldRead.resolve(message({ author: author('detached_author') }));
      await oldAttempt;
      await f.actions.profile(message());
      assert.equal(f.reads.length, 2, 'Detached finally cannot release the new attempt');
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.alerts, []);
      newRead.resolve(message({ author: author('restored_author') }));
      await newAttempt;
      assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: 'restored_author' } }]);
    });
  }
});

test('source-chat and disabled-state round trips detach old reads before a new attempt', async () => {
  for (const transition of ['source', 'disabled'] as const) {
    const f = fixture();
    f.update((state) => ({
      ...state,
      snapshot: {
        ...state.snapshot!,
        conversations: [
          ...state.snapshot!.conversations,
          { ...state.snapshot!.conversations[0]!, id: 'other_chat' },
        ],
      },
    }));
    const oldRead = deferred<TimelineEntry>();
    const newRead = deferred<TimelineEntry>();
    f.read(() => oldRead.promise);
    const oldAttempt = f.actions.profile(message());
    if (transition === 'source') f.render({ spaceId: 'other_chat' });
    else f.render({ disabled: true });
    const actions = f.render({ spaceId, disabled: false });
    f.read(() => newRead.promise);
    const newAttempt = actions.profile(message());
    oldRead.reject(new Error('Synthetic detached round-trip read failure'));
    await oldAttempt;
    await actions.profile(message());
    assert.equal(f.reads.length, 2);
    assert.deepEqual(f.alerts, []);
    assert.deepEqual(f.routes, []);
    newRead.resolve(message());
    await newAttempt;
    assert.equal(f.routes.length, 1);
  }
});

test('canonical read cannot reopen a row disabled, deleted or removed while the read was pending', async () => {
  for (const patch of [
    { disabled: true },
    { entries: [] },
    { entries: [message({ deleted: true })] },
  ]) {
    const f = fixture();
    const read = deferred<TimelineEntry>();
    f.read(() => read.promise);
    const pending = f.actions.profile(message());
    f.render(patch);
    read.resolve(message());
    await pending;
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.alerts, []);
  }
});

test('malformed or noninteractive canonical results report the failure and permit a valid retry', async () => {
  const invalid = [
    message({ id: 'different_message' }),
    message({ deleted: true }),
    message({ kind: { System: { text: 'Synthetic system event' } } }),
    message({ author: author('   ') }),
    { ...message(), author: null } as unknown as TimelineEntry,
  ];
  for (const canonical of invalid) {
    const f = fixture();
    f.read(async () => canonical);
    await f.actions.profile(message());
    assert.deepEqual(f.routes, []);
    assert.equal(f.alerts.length, 1);
    assert.equal(f.alerts[0]![0], 'chat.profileUnavailable');
    f.read(async () => message({ author: author('valid_retry') }));
    await f.actions.profile(message());
    assert.equal(f.reads.length, 2);
    assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: 'valid_retry' } }]);
  }
});

test('read errors alert only the active attempt and a same-focus retry is available', async () => {
  const f = fixture();
  f.read(async () => {
    throw new Error('Synthetic profile read failure');
  });
  await f.actions.profile(message());
  assert.equal(f.alerts.length, 1);
  assert.equal(f.alerts[0]![1], 'Synthetic profile read failure');
  f.read(async () => message());
  await f.actions.profile(message());
  assert.equal(f.routes.length, 1);
  assert.equal(f.reads.length, 2);
  for (const lifecycle of ['blur', 'unmount'] as const) {
    const stale = fixture();
    const read = deferred<TimelineEntry>();
    stale.read(() => read.promise);
    const pending = stale.actions.profile(message());
    stale[lifecycle]();
    read.reject(new Error('Synthetic detached read failure'));
    await pending;
    assert.deepEqual(stale.alerts, []);
    assert.deepEqual(stale.routes, []);
  }
});

test('an old success or failure after blur/refocus cannot navigate or clear the newer attempt latch', async () => {
  for (const outcome of ['resolve', 'reject'] as const) {
    const f = fixture();
    const oldRead = deferred<TimelineEntry>();
    const newRead = deferred<TimelineEntry>();
    f.read(() => oldRead.promise);
    const oldAttempt = f.actions.profile(message());
    f.blur();
    f.focus();
    f.read(() => newRead.promise);
    const actions = f.render();
    const newAttempt = actions.profile(message());
    if (outcome === 'resolve') oldRead.resolve(message({ author: author('stale_author') }));
    else oldRead.reject(new Error('Synthetic stale read failure'));
    await oldAttempt;
    await actions.profile(message());
    assert.equal(f.reads.length, 2, 'Old finally must not release the active attempt');
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.alerts, []);
    newRead.resolve(message({ author: author('fresh_author') }));
    await newAttempt;
    assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: 'fresh_author' } }]);
  }
});

test('a synchronous router failure releases the attempt for retry without a second canonical mutation', async () => {
  const f = fixture();
  f.push(() => {
    throw new Error('Synthetic unavailable navigator');
  });
  await f.actions.profile(message());
  assert.deepEqual(f.routes, []);
  assert.equal(f.alerts.length, 1);
  assert.equal(f.alerts[0]![1], 'Synthetic unavailable navigator');
  f.push(() => {});
  await f.actions.profile(message());
  assert.equal(f.routes.length, 1);
  assert.equal(f.reads.length, 2);
  assert.deepEqual(f.mutations, []);
});

test('an in-flight message mutation defers author navigation until the mutation completes', async () => {
  const f = fixture();
  const mutation = deferred<unknown>();
  f.mutate(() => mutation.promise);
  const reaction = f.actions.react(message(), '👍');
  await f.actions.profile(message());
  assert.deepEqual(f.reads, []);
  mutation.resolve(undefined);
  await reaction;
  await f.actions.profile(message());
  assert.equal(f.routes.length, 1);
  assert.equal(f.mutations.length, 1);
});
