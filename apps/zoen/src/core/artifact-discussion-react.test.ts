import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred } from './store.fixtures.ts';
import type { ItemDiscussionDto } from './generated/ItemDiscussionDto';
import { discussionReactFixture } from './artifact-discussion-react.fixtures.ts';

test('compiled ArtifactDiscussion appears after a closed account first opens', async () => {
  const f = await discussionReactFixture(false);
  try {
    await f.render();
    assert.equal(f.button(), null);
    await f.open();
    assert.ok(f.button());
    assert.equal(f.button()!.disabled, false);
    await f.act(() => f.button()!.onPress());
    assert.equal(f.routes.length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
for (const change of [undefined, 'identity', 'device', 'relay'] as const)
  test(`compiled ArtifactDiscussion follows close/reopen: ${change ?? 'same IDs/new epoch'}`, async () => {
    const f = await discussionReactFixture();
    try {
      await f.render();
      const old = f.button();
      assert.ok(old);
      assert.equal(old.disabled, false);
      await f.act(() => old.onPress());
      assert.equal(f.routes.length, 1);
      await f.reopen(change);
      const current = f.button();
      assert.ok(current);
      assert.equal(current.disabled, false);
      await f.act(() => old.onPress());
      assert.equal(f.routes.length, 1, 'old activation must never open another session');
      await f.act(() => current.onPress());
      assert.equal(f.routes.length, 2, 'fresh explicit button must use current authority');
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
test('compiled ArtifactDiscussion disappears synchronously on canonical conversation loss', async () => {
  const f = await discussionReactFixture();
  try {
    await f.render();
    const old = f.button();
    assert.ok(old);
    f.snapshot.conversations = [];
    await f.act(() => f.store.refresh());
    assert.equal(f.button(), null, 'retired discussion must not keep its previous reply count');
    await f.act(() => old.onPress());
    assert.equal(f.routes.length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled ArtifactDiscussion ignores old query completion after current account reopens', async () => {
  const f = await discussionReactFixture(),
    gate = deferred<ItemDiscussionDto>();
  try {
    const oldValue = f.value();
    f.read(gate.promise);
    await f.render();
    assert.equal(f.button()?.disabled, true);
    await f.reopen('identity');
    const current = f.button();
    assert.ok(current);
    assert.equal(current.disabled, false);
    await f.act(() =>
      gate.resolve({ ...oldValue, root: { ...oldValue.root!, thread_replies: 99 } }),
    );
    assert.equal(
      f.button(),
      current,
      'retired query must not replace the current control or count',
    );
    await f.act(() => current.onPress());
    assert.equal(f.routes.length, 1);
    await f.act(() => current.onPress());
    assert.equal(f.routes.length, 1, 'same visible activation opens only once');
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve(f.value());
    await f.close();
  }
});
