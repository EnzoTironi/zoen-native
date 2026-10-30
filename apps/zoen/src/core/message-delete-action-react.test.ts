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
  const answer = deferred<boolean>();
  f.confirm(() => answer.promise);
  action(own);
  action(own);
  assert.equal(f.confirmations.length, 1);
  assert.equal(f.confirmations[0]!.destructive, true);
  assert.equal(f.render().busy, true);
  assert.deepEqual(f.mutations, []);
  answer.resolve(true);
  await settle();
  assert.deepEqual(JSON.parse(JSON.stringify(f.mutations)), [
    {
      op: 'delete_messages',
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
    const answer = deferred<boolean>();
    f.confirm(() => answer.promise);
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
    answer.resolve(change !== 'cancel');
    await settle();
    assert.deepEqual(f.mutations, []);
    f.unmount();
  });

test('a failed individual delete releases the action lock and requires fresh confirmation to retry', async () => {
  const { f, own, action } = await opened();
  f.confirm(async () => true);
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
  f.confirm(async () => true);
  f.store.sessionEpoch++;
  action(own);
  await settle();
  assert.deepEqual(f.confirmations, []);
  assert.deepEqual(f.mutations, []);
  f.unmount();
});

for (const reason of ['reader', 'locked', 'foreign', 'missing-revision'] as const)
  test(`the current single-message menu omits deletion for ${reason}`, async () => {
    const { f, own } = await opened();
    if (reason === 'reader')
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
    if (reason === 'locked') f.update(account({ unlocked: false }));
    const entry =
      reason === 'foreign'
        ? { ...own, author: author('other') }
        : reason === 'missing-revision'
          ? { ...own, text_revision: undefined }
          : own;
    assert.equal(f.render({ entries: [entry] }).panel.props.onDelete, undefined);
    assert.deepEqual(f.confirmations, []);
    assert.deepEqual(f.mutations, []);
    f.unmount();
  });
