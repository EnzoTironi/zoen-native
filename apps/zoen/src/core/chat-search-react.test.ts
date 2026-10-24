import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chatSearchReactFixture } from './chat-search-react.fixtures.ts';

for (const change of [undefined, 'relay'] as const)
  test(`Expo chat retires search on reopen: ${change ?? 'same owner/new epoch'}`, async () => {
    const f = await chatSearchReactFixture();
    try {
      await f.render();
      await f.act(() => {
        f.current().search.open();
        f.current().search.setQuery('private prior query');
      });
      const old = f.current().search;
      await f.reopen(change);
      assert.equal(
        f.current().searchState.query,
        '',
        'retired query must not remain on the reopened chat',
      );
      assert.equal(f.current().searchState.active, false);
      assert.equal(old.isCurrent(), false);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('Expo chat rejects a prior-epoch search response after same-owner reopen', async () => {
  const f = await chatSearchReactFixture();
  try {
    await f.render();
    await f.act(() => {
      f.current().search.open();
      f.current().search.setQuery('private query');
    });
    const old = f.current().search;
    let read: Promise<void>;
    await f.act(() => {
      read = old.search();
    });
    await f.reopen();
    await f.act(async () => {
      f.pending.resolve({
        hits: [{ kind: 'message', ref_id: 'old-private-message', space_id: 'read-chat' } as any],
        next_cursor: null,
      });
      await read!;
    });
    assert.equal(f.current().searchState.hits.length, 0);
    assert.equal(old.getSnapshot().hits.length, 0);
    assert.equal(
      f.calls.filter((c) => c.op === 'message').length,
      0,
      'late old hit must not trigger another read',
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('Expo chat clears private search presentation when membership is revoked', async () => {
  const f = await chatSearchReactFixture();
  try {
    await f.render();
    await f.act(() => {
      f.current().search.open();
      f.current().search.setQuery('private prior query');
    });
    const old = f.current().search;
    f.snapshot.conversations[0]!.members = [];
    await f.act(() => f.store.refresh());
    assert.equal(f.current().searchState.query, '');
    assert.equal(f.current().searchState.active, false);
    assert.equal(old.isCurrent(), false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const restored of [false, true])
  test(`Expo chat discards late search hits after membership loss, restored=${restored}`, async () => {
    const f = await chatSearchReactFixture();
    try {
      await f.render();
      await f.act(() => {
        f.current().search.open();
        f.current().search.setQuery('private query');
      });
      const old = f.current().search;
      let read: Promise<void>;
      await f.act(() => {
        read = old.search();
      });
      const members = f.snapshot.conversations[0]!.members;
      await f.act(async () => {
        f.snapshot.conversations[0]!.members = [];
        await f.store.refresh();
        if (restored) {
          f.snapshot.conversations[0]!.members = members;
          await f.store.refresh();
        }
      });
      await f.act(async () => {
        f.pending.resolve({
          hits: [{ kind: 'message', ref_id: 'revoked-hit', space_id: 'read-chat' } as any],
          next_cursor: null,
        });
        await read!;
      });
      assert.equal(f.current().searchState.active, false);
      assert.equal(f.current().searchState.query, '');
      assert.equal(f.current().searchState.hits.length, 0);
      assert.equal(old.getSnapshot().hits.length, 0);
      assert.equal(f.calls.filter((c) => c.op === 'message').length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('Expo chat preserves active search across unchanged canonical refresh', async () => {
  const f = await chatSearchReactFixture();
  try {
    await f.render();
    await f.act(() => {
      f.current().search.open();
      f.current().search.setQuery('current query');
    });
    const search = f.current().search;
    await f.act(() => f.store.refresh());
    assert.equal(f.current().search, search);
    assert.equal(f.current().searchState.active, true);
    assert.equal(f.current().searchState.query, 'current query');
    assert.equal(search.isCurrent(), true);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
