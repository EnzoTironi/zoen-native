import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';
const settle = () => new Promise((resolve) => setTimeout(resolve, 240));
const hit = (id: string) => ({
  kind: 'message',
  space_id: 'chat',
  ref_id: id,
  title: id,
  snippet: id,
});

for (const change of ['session', 'relay'] as const) {
  test(`the optimized search route accepts a fresh query after ${change} replacement without restoring old text`, async () => {
    const f = await fixture(
      'private original',
      'account',
      { hits: [hit('old')], next_cursor: null },
      'ios',
      true,
    );
    try {
      const oldInput = f.input;
      if (change === 'session') {
        await f.root.act(() => f.store.close());
        await f.render();
        await f.root.act(() => f.store.open('account'));
      } else {
        f.canonicalSnapshot.account.relay_url = 'https://replacement.invalid';
        await f.root.act(() => f.store.refresh());
      }
      await f.render();
      assert.equal(f.input.value, '');
      assert.equal(f.list.data.length, 0);
      f.searchRead(async () => ({ hits: [hit('fresh result')], next_cursor: null }));
      await f.root.act(() => f.input.onChangeText('fresh query'));
      await f.root.act(settle);
      assert.equal(f.input.value, 'fresh query');
      assert.equal(f.queries.at(-1).query, 'fresh query');
      assert.equal(f.list.data[0]?.ref_id, 'fresh result');
      const reads = f.queries.length;
      await f.root.act(() => oldInput.onChangeText('retired text'));
      await f.root.act(settle);
      assert.equal(f.input.value, 'fresh query');
      assert.equal(f.queries.length, reads);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.root.close();
      await f.store.close();
    }
  });
}

test('a mounted scoped search recovers after canonical access is removed and restored', async () => {
  const f = await fixture(
    'needle',
    'account',
    { hits: [hit('original')], next_cursor: null },
    'ios',
    true,
    { space: 'chat' },
  );
  try {
    const chats = f.canonicalSnapshot.conversations;
    f.canonicalSnapshot.conversations = [];
    await f.root.act(() => f.store.refresh(['chat']));
    await f.render();
    assert.equal(f.list.data.length, 0);
    const reads = f.queries.length;
    await f.root.act(() => f.input.onChangeText('forbidden'));
    await f.root.act(settle);
    assert.equal(f.queries.length, reads);
    f.canonicalSnapshot.conversations = chats;
    await f.root.act(() => f.store.refresh(['chat']));
    await f.render();
    f.searchRead(async () => ({ hits: [hit('restored')], next_cursor: null }));
    await f.root.act(() => f.input.onChangeText('authorized'));
    await f.root.act(settle);
    assert.equal(f.queries.at(-1).query, 'authorized');
    assert.equal(f.queries.at(-1).messages.space_id, 'chat');
    assert.equal(f.list.data[0]?.ref_id, 'restored');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
    await f.store.close();
  }
});

test('visible conversation filters follow canonical membership additions and removals', async () => {
  const f = await fixture('needle', 'account', { hits: [], next_cursor: null }, 'ios', true);
  try {
    const original = f.canonicalSnapshot.conversations[0];
    f.canonicalSnapshot.conversations.push({ ...original, id: 'new-chat', title: 'New chat' });
    await f.root.act(() => f.store.refresh());
    await f.render();
    assert.equal(
      f.identities.options.spaces.some((row: { id: string }) => row.id === 'new-chat'),
      true,
    );
    f.canonicalSnapshot.conversations = [original];
    await f.root.act(() => f.store.refresh());
    await f.render();
    assert.equal(
      f.identities.options.spaces.some((row: { id: string }) => row.id === 'new-chat'),
      false,
    );
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
    await f.store.close();
  }
});
