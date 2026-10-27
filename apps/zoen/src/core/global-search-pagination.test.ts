import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';
import { deferred } from './store.fixtures.ts';

const hit = (id: number) => ({
  kind: 'message',
  space_id: 'chat',
  ref_id: `message-${id}`,
  title: `Match ${id}`,
  snippet: 'needle',
});
const first = () => ({
  hits: Array.from({ length: 30 }, (_, n) => hit(n)),
  next_cursor: 'opaque-page-2',
});
const more = (f: Awaited<ReturnType<typeof fixture>>) =>
  f.list.ListFooterComponent.props.children.props;
const normalized = (value: unknown) => JSON.parse(JSON.stringify(value));

test('compiled global search exposes results beyond30 using the canonical cursor and retains them after a focused return', async () => {
  const f = await fixture('needle', 'account', first());
  try {
    assert.equal(f.list.data.length, 30);
    assert.ok(
      f.list.ListFooterComponent,
      'The first30 global hits must expose their canonical continuation',
    );
    assert.deepEqual(normalized(f.queries[0]), {
      op: 'universal_search_page',
      query: 'needle',
      kinds: [],
      cursor: null,
      limit: 30,
      expected_identity_id: 'me',
      expected_device_id: 'device',
    });
    f.searchRead(async () => ({
      hits: Array.from({ length: 16 }, (_, n) => hit(n + 29)),
      next_cursor: null,
    }));
    await f.root.act(() => more(f).onPress());
    assert.equal(f.list.data.length, 45);
    assert.equal(f.list.data[30].ref_id, 'message-30');
    assert.equal(f.queries[1].cursor, 'opaque-page-2');
    assert.equal(f.list.ListFooterComponent, null);
    await f.root.act(() => f.focus(false));
    await f.root.act(() => f.focus(true));
    assert.equal(f.input.value, 'needle');
    assert.equal(f.list.data.length, 45);
    assert.equal(
      f.queries.length,
      2,
      'Return keeps the accepted pages instead of requerying page one',
    );
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});

for (const retirement of [
  'query',
  'blur',
  'focus-ABA',
  'owner',
  'device',
  'relay',
  'access-ABA',
  'unmount',
] as const)
  test(`compiled late global page cannot append after ${retirement}`, async () => {
    const f = await fixture('needle', 'account', first());
    try {
      const pending = deferred<any>();
      f.searchRead(() => pending.promise);
      const old = more(f);
      await f.root.act(() => old.onPress());
      if (retirement === 'query') await f.root.act(() => f.input.onChangeText('another'));
      else if (retirement === 'blur') await f.root.act(() => f.focus(false));
      else if (retirement === 'focus-ABA')
        await f.root.act(() => {
          f.focus(false);
          f.focus(true);
        });
      else if (retirement === 'device' || retirement === 'relay') {
        if (retirement === 'device') f.state.snapshot.account.device_id = 'new-device';
        else f.state.snapshot.account.relay_url = 'https://other-relay.example';
        await f.render();
      } else if (retirement === 'owner') {
        f.store.sessionEpoch++;
        await f.render();
      } else if (retirement === 'unmount') await f.root.render(null);
      else
        await f.root.act(() => {
          f.revoke();
          f.state.snapshot.conversations = [{ id: 'chat', members: [{ id: 'me' }] }];
        });
      await f.root.act(async () => {
        pending.resolve({ hits: [hit(999)], next_cursor: null });
        await pending.promise;
        old.onPress();
      });
      assert.equal(
        f.list.data.some((row: any) => row.ref_id === 'message-999'),
        false,
      );
      assert.equal(f.queries.length, 2, 'Retained More cannot adopt the next query/focus/owner');
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.root.close();
    }
  });

test('out-of-order first-page reads publish only the latest query', async () => {
  const f = await fixture('needle', 'account', first());
  try {
    const old = deferred<any>();
    f.searchRead((command) =>
      command.query === 'old'
        ? old.promise
        : Promise.resolve({ hits: [hit(200)], next_cursor: null }),
    );
    await f.root.act(() => f.input.onChangeText('old'));
    await f.root.act(() => new Promise((resolve) => setTimeout(resolve, 220)));
    await f.root.act(() => f.input.onChangeText('new'));
    await f.root.act(() => new Promise((resolve) => setTimeout(resolve, 220)));
    await f.root.act(async () => {
      old.resolve({ hits: [hit(100)], next_cursor: 'wrong' });
      await old.promise;
    });
    assert.equal(f.input.value, 'new');
    assert.deepEqual(
      Array.from(f.list.data, (row: any) => row.ref_id),
      ['message-200'],
    );
    assert.equal(f.list.ListFooterComponent, null);
  } finally {
    await f.root.close();
  }
});

test('stale cursor exposes failure and explicit retry restarts the same query at page one', async () => {
  const f = await fixture('needle', 'account', first());
  try {
    f.searchRead(async (command) => {
      if (command.cursor) throw Error('Index changed. Restart search.');
      return { hits: [hit(55)], next_cursor: null };
    });
    await f.root.act(() => more(f).onPress());
    assert.equal(f.list.data.length, 30);
    assert.equal(more(f).disabled, true);
    assert.ok(f.list.ListHeaderComponent);
    await f.root.act(() => f.list.ListHeaderComponent.props.children[1].props.onPress());
    assert.equal(f.queries.at(-1).cursor, null);
    assert.equal(f.queries.at(-1).query, 'needle');
    assert.deepEqual(
      Array.from(f.list.data, (row: any) => row.ref_id),
      ['message-55'],
    );
  } finally {
    await f.root.close();
  }
});

test('native demo retains the existing bounded search without inventing account credentials or a cursor', async () => {
  const f = await fixture('needle', 'demo', {
    hits: [hit(1)],
    next_cursor: 'not-a-demo-capability',
  });
  try {
    assert.equal(f.queries[0].op, 'universal_search');
    assert.equal(f.queries[0].expected_identity_id, undefined);
    assert.equal(f.list.ListFooterComponent, null);
  } finally {
    await f.root.close();
  }
});

test('a result on the second page resolves its exact canonical thread and keeps all pages on return', async () => {
  const f = await fixture('needle', 'account', first());
  try {
    f.searchRead(async () => ({ hits: [hit(30)], next_cursor: null }));
    await f.root.act(() => more(f).onPress());
    await f.root.act(() => f.list.renderItem({ item: f.list.data[30] }).props.onPress());
    assert.deepEqual(normalized(f.reads), [
      { op: 'message', space_id: 'chat', message_id: 'message-30' },
    ]);
    await f.root.act(async () => {
      f.pending.resolve({ id: 'message-30', in_thread: 'thread-root', deleted: false });
      await f.pending.promise;
    });
    assert.deepEqual(normalized(f.routes), [
      { pathname: '/thread/[space]/[root]', params: { space: 'chat', root: 'thread-root' } },
    ]);
    await f.root.act(() => f.focus(false));
    await f.root.act(() => f.focus(true));
    assert.equal(f.input.value, 'needle');
    assert.equal(f.list.data.length, 31);
    assert.equal(f.queries.length, 2);
  } finally {
    await f.root.close();
  }
});

test('browser account preserves its existing search authority without requesting a native-only command', async () => {
  const f = await fixture('needle', 'account', { hits: [hit(1)] }, 'web');
  try {
    assert.equal(f.queries[0].op, 'universal_search');
    assert.equal(f.list.data.length, 1);
    assert.equal(f.list.ListFooterComponent, null);
  } finally {
    await f.root.close();
  }
});

test('an oversized UTF8 query shows the existing bounded-search error instead of waiting forever', async () => {
  const f = await fixture('😀'.repeat(129), 'account');
  try {
    assert.equal(f.queries.length, 0);
    assert.ok(f.list.ListHeaderComponent);
    assert.equal(f.list.ListHeaderComponent.props.children[1].props.disabled, true);
    assert.equal(f.list.ListEmptyComponent, null);
  } finally {
    await f.root.close();
  }
});

test('browser keeps its prior long-query semantics rather than inheriting the native FTS byte limit', async () => {
  const query = '😀'.repeat(129);
  const f = await fixture(query, 'account', { hits: [hit(1)] }, 'web');
  try {
    assert.equal(f.queries.length, 1);
    assert.equal(f.queries[0].query, query);
    assert.equal(f.list.data.length, 1);
  } finally {
    await f.root.close();
  }
});
