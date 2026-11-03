import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  account,
  author,
  deferred,
  fixture,
  message,
  spaceId,
} from './message-author-navigation.fixtures.ts';

const ownMessage = () =>
  message({ author: { ...author('alice'), is_me: true }, text_revision: 'source-v1' });
const settle = () => new Promise<void>((done) => setImmediate(done));
async function opened() {
  const f = fixture();
  const own = ownMessage();
  f.read(async () => own);
  await f.render({ entries: [own] }).open(own, 'actions');
  return { f, own, action: f.render().panel.props.onDelete! };
}

test('the compiled single-message menu uses the reviewed batch command and prevents duplicate confirmations', async () => {
  const { f, own, action } = await opened();
  const answer = deferred<string | null>();
  f.choose(() => answer.promise);
  action(own);
  action(own);
  assert.equal(f.confirmations.length, 1);
  assert.ok(f.confirmations[0]!.choices.every((choice) => choice.destructive));
  assert.equal(f.render().busy, true);
  assert.deepEqual(f.mutations, []);
  answer.resolve('everyone');
  await settle();
  assert.deepEqual(JSON.parse(JSON.stringify(f.mutations)), [
    {
      op: 'delete_messages',
      for_me: false,
      expected_identity_id: 'alice',
      expected_device_id: 'phone',
      space_id: spaceId,
      targets: [{ message_id: own.id, expected_revision: 'source-v1' }],
    },
  ]);
  assert.equal(f.render().busy, false);
  f.unmount();
});

for (const change of [
  'cancel',
  'edit',
  'deleted',
  'blur',
  'focus-ABA',
  'owner',
  'device',
  'epoch',
  'retiring',
  'reader',
  'revocation-ABA',
  'unmount',
] as const)
  test(`an individual deletion confirmation retires after ${change}`, async () => {
    const { f, own, action } = await opened();
    const answer = deferred<string | null>();
    f.choose(() => answer.promise);
    action(own);
    if (change === 'edit') f.render({ entries: [{ ...own, text_revision: 'source-v2' }] });
    if (change === 'deleted') f.render({ entries: [{ ...own, deleted: true }] });
    if (change === 'blur' || change === 'focus-ABA') f.blur();
    if (change === 'focus-ABA') {
      f.focus();
      f.render();
    }
    if (change === 'owner') f.update(account({ identity_id: 'other' }));
    if (change === 'device') f.update(account({ device_id: 'other-phone' }));
    if (change === 'epoch') f.store.sessionEpoch++;
    if (change === 'retiring') f.store.isSessionRetiring = true;
    if (change === 'reader')
      f.update((state) => ({
        ...state,
        snapshot: {
          ...state.snapshot!,
          conversations: state.snapshot!.conversations.map((chat) => ({
            ...chat,
            can_write: false,
          })),
        },
      }));
    if (change === 'revocation-ABA') {
      const prior = f.state();
      f.update((state) => ({ ...state, snapshot: { ...state.snapshot!, conversations: [] } }));
      f.update(() => prior);
    }
    if (change === 'unmount') f.unmount();
    answer.resolve(change === 'cancel' ? null : 'everyone');
    await settle();
    assert.deepEqual(f.mutations, []);
    f.unmount();
  });

test('a failed individual delete releases the action lock and requires fresh confirmation to retry', async () => {
  const { f, own, action } = await opened();
  f.choose(async () => 'everyone');
  f.mutate(async () => {
    throw new Error('storage unavailable');
  });
  action(own);
  await settle();
  assert.equal(f.render().busy, false);
  f.mutate(async () => []);
  f.render().panel.props.onDelete!(own);
  await settle();
  assert.equal(f.confirmations.length, 2);
  assert.equal(f.mutations.length, 2);
  assert.equal(f.render().busy, false);
  f.unmount();
});

test('held delete controls cannot start a confirmation in a new same-owner session', async () => {
  const { f, own, action } = await opened();
  f.choose(async () => 'everyone');
  f.store.sessionEpoch++;
  action(own);
  await settle();
  assert.deepEqual(f.confirmations, []);
  assert.deepEqual(f.mutations, []);
  f.unmount();
});

for (const reason of ['locked', 'missing-revision'] as const)
  test(`the current single-message menu omits deletion for ${reason}`, async () => {
    const { f, own } = await opened();
    if (reason === 'locked') f.update(account({ unlocked: false }));
    const entry = reason === 'missing-revision' ? { ...own, text_revision: undefined } : own;
    assert.equal(f.render({ entries: [entry] }).panel.props.onDelete, undefined);
    assert.deepEqual(f.confirmations, []);
    assert.deepEqual(f.mutations, []);
    f.unmount();
  });

for (const reader of [false, true])
  test(`received messages offer only private deletion, including reader=${reader}`, async () => {
    const f = fixture();
    const received = message({ text_revision: 'received-v1' });
    if (reader)
      f.update((state) => ({
        ...state,
        snapshot: {
          ...state.snapshot!,
          conversations: state.snapshot!.conversations.map((chat) => ({
            ...chat,
            can_write: false,
          })),
        },
      }));
    f.read(async () => received);
    await f.render({ entries: [received] }).open(received, 'actions');
    f.choose(async (options) => {
      assert.deepEqual(
        Array.from(options.choices, (choice) => choice.id),
        ['for_me'],
      );
      return 'for_me';
    });
    f.render().panel.props.onDelete!(received);
    await settle();
    assert.equal(f.mutations.length, 1);
    assert.deepEqual(JSON.parse(JSON.stringify(f.mutations[0])), {
      op: 'delete_messages',
      for_me: true,
      expected_identity_id: 'alice',
      expected_device_id: 'phone',
      space_id: spaceId,
      targets: [{ message_id: received.id, expected_revision: 'received-v1' }],
    });
    f.unmount();
  });

test('the native confirmation is aborted on blur and cannot outlive the conversation', async () => {
  const { f, own, action } = await opened();
  let signal: AbortSignal | undefined;
  const answer = deferred<string | null>();
  f.choose((options) => {
    signal = options.signal;
    return answer.promise;
  });
  action(own);
  assert.equal(signal?.aborted, false);
  f.blur();
  assert.equal(signal?.aborted, true);
  answer.resolve('for_me');
  await settle();
  assert.deepEqual(f.mutations, []);
  f.unmount();
});

test('a redacted thread parent opens its remaining replies without enabling reactions or deletion', async () => {
  const f = fixture();
  const root = message({
    deleted: true,
    thread_replies: 2,
    kind: { Message: { text: '', card: null } },
  });
  const hook = f.render({ entries: [root] });
  hook.thread(root);
  assert.equal(f.routes.length, 1);
  assert.deepEqual(JSON.parse(JSON.stringify(f.routes[0])), {
    pathname: '/thread/[space]/[root]',
    params: { space: spaceId, root: root.id },
  });
  await hook.open(root, 'reactions');
  assert.equal(f.render().panel.props.entry, null);
  assert.deepEqual(f.mutations, []);
  f.unmount();
});

test('a deleted leaf offers device-only removal using its stable identity as the terminal revision', async () => {
  const f = fixture();
  const removed = message({
    deleted: true,
    text_revision: undefined,
    thread_replies: 0,
    kind: { Message: { text: '', card: null } },
  });
  await f.render({ entries: [removed] }).open(removed, 'actions');
  const menu = f.render().panel.props;
  assert.equal(typeof menu.onDelete, 'function');
  for (const name of ['onEdit', 'onForward', 'onCopy', 'onInfo', 'onShare'] as const)
    assert.equal(menu[name], undefined, name);
  f.choose(async (options) => {
    assert.deepEqual(
      Array.from(options.choices, (choice) => choice.id),
      ['for_me'],
    );
    return 'for_me';
  });
  menu.onDelete!(removed);
  await settle();
  assert.deepEqual(JSON.parse(JSON.stringify(f.mutations)), [
    {
      op: 'delete_messages',
      for_me: true,
      expected_identity_id: 'alice',
      expected_device_id: 'phone',
      space_id: spaceId,
      targets: [{ message_id: removed.id, expected_revision: removed.id }],
    },
  ]);
  f.unmount();
});

for (const change of ['edited', 'removed', 'new-thread-reply'] as const)
  test(`a retained menu cannot reacquire a changed source after ${change}`, async () => {
    const { f, own, action } = await opened();
    f.choose(async () => 'for_me');
    const updated =
      change === 'edited'
        ? { ...own, text_revision: 'edited-source' }
        : {
            ...own,
            deleted: true,
            text_revision: undefined,
            thread_replies: change === 'new-thread-reply' ? 1 : 0,
          };
    f.render({ entries: [updated] });
    action(own);
    await settle();
    assert.deepEqual(f.confirmations, []);
    assert.deepEqual(f.mutations, []);
    f.unmount();
  });
