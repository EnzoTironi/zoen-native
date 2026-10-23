import assert from 'node:assert/strict';
import test from 'node:test';
import type { Snapshot, TimelineEntry } from '../core/types';
import type { ItemDiscussionDto } from '../core/generated/ItemDiscussionDto';
import { discussionScope, discussionTarget } from './discussion.ts';

type Session = Parameters<typeof discussionScope>[0];
function session(): Session {
  return {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'alice', device_id: 'phone', unlocked: true, setup_pending: false },
      me: { id: 'alice' },
      conversations: [{ id: 'chat', members: [{ id: 'alice' }] }],
    } as unknown as Snapshot,
  };
}
const discussion = (): ItemDiscussionDto => ({
  item_id: 'page',
  space_id: 'chat',
  root: { id: 'original-creation', in_thread: null } as TimelineEntry,
});
const target = (
  state: Session,
  result = discussion(),
  epoch = 1,
  expected = discussionScope(session(), 1)!,
) => discussionTarget(state, epoch, expected, 'chat', 'page', result);

test('artifact comments navigate to their canonical original thread', () => {
  assert.deepEqual(target(session()), { space: 'chat', root: 'original-creation' });
  const result = discussion();
  result.root!.deleted = true;
  assert.deepEqual(target(session(), result), { space: 'chat', root: 'original-creation' });
});

test('a late discussion read cannot navigate after identity, device or session replacement', () => {
  for (const field of ['identity_id', 'device_id'] as const) {
    const state = session();
    state.snapshot!.account![field] = 'different';
    assert.equal(target(state), null);
  }
  assert.equal(target(session(), discussion(), 2), null);
  const demo = session();
  demo.mode = 'demo';
  assert.equal(target(demo), null);
});

test('locked, erasing, opening and revoked memberships cannot open cached comments', () => {
  for (const condition of ['locked', 'erasing', 'opening', 'revoked'] as const) {
    const state = session();
    if (condition === 'locked') state.snapshot!.account!.unlocked = false;
    if (condition === 'erasing') state.erasePending = true;
    if (condition === 'opening') state.phase = 'opening';
    if (condition === 'revoked') state.snapshot!.conversations[0]!.members = [];
    assert.equal(target(state), null);
  }
});

test('results cannot substitute another artifact, chat or a nested reply as the root', () => {
  const foreignItem = discussion();
  foreignItem.item_id = 'other';
  const foreignSpace = discussion();
  foreignSpace.space_id = 'other';
  const nested = discussion();
  nested.root!.in_thread = 'other-root';
  const missing = discussion();
  missing.root = null;
  for (const result of [foreignItem, foreignSpace, nested, missing])
    assert.equal(target(session(), result), null);
});
