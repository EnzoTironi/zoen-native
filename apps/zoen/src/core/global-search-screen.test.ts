import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';

test('compiled search keeps empty/loading/error states inside its keyboard-safe list', async () => {
  const f = await fixture();
  try {
    assert.equal(f.avoiding.behavior, 'padding');
    assert.equal(f.avoiding.automaticOffset, true);
    assert.equal(f.list.keyboardShouldPersistTaps, 'handled');
    assert.equal(f.list.keyboardDismissMode, 'interactive');
    assert.equal(f.list.contentContainerStyle.flexGrow, 1);
    assert.equal(f.list.contentContainerStyle.paddingBottom, 50);
    assert.equal(f.list.ListEmptyComponent.type, f.EmptyState);
    await f.root.act(() => f.input.onChangeText('missing'));
    assert.equal(f.list.ListEmptyComponent.type, f.Loading);
    await f.result(null, 'offline');
    await f.root.act(() => new Promise((resolve) => setTimeout(resolve, 210)));
    assert.ok(f.list.ListHeaderComponent);
    assert.equal(f.list.ListEmptyComponent, null);
    await f.root.act(() => f.list.ListHeaderComponent.props.children[1].props.onPress());
    assert.equal(f.retries, 1);
    await f.result({ hits: [] });
    await f.root.act(() => f.list.ListHeaderComponent.props.children[1].props.onPress());
    assert.equal(f.list.ListEmptyComponent.props.title, 'chat.noResults');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});

test('compiled result opening preserves query and resolves the canonical thread before navigation', async () => {
  const f = await fixture('old query');
  try {
    await f.root.act(() => f.list.renderItem({ item: f.list.data[0] }).props.onPress());
    assert.equal(f.reads.length, 1);
    await f.root.act(async () => {
      f.pending.resolve({ id: 'message', in_thread: 'root', deleted: false });
      await f.pending.promise;
    });
    assert.equal(f.input.value, 'old query');
    assert.deepEqual(JSON.parse(JSON.stringify(f.routes)), [
      { pathname: '/thread/[space]/[root]', params: { space: 'chat', root: 'root' } },
    ]);
    assert.equal(f.dismissals, 1);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});

for (const retirement of ['query', 'unmount', 'account', 'access-ABA'] as const)
  test(`compiled result cannot navigate or dismiss keyboard after ${retirement}`, async () => {
    const f = await fixture('old query');
    try {
      await f.root.act(() => f.list.renderItem({ item: f.list.data[0] }).props.onPress());
      if (retirement === 'query') await f.root.act(() => f.input.onChangeText('new query'));
      else if (retirement === 'unmount') await f.root.render(null);
      else if (retirement === 'account') {
        f.store.sessionEpoch++;
        await f.render();
      } else {
        await f.root.act(() => f.revoke());
        f.state.snapshot.conversations = [{ id: 'chat', members: [{ id: 'me' }] }];
      }
      await f.root.act(async () => {
        f.pending.resolve({ id: 'message', in_thread: null });
        await f.pending.promise;
      });
      assert.equal(f.routes.length, 0);
      assert.equal(f.dismissals, 0);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.root.close();
    }
  });
