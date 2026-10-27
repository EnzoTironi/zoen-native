import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';
import { deferred } from './store.fixtures.ts';

const hit = (id: number, snippet = 'original text') => ({
  kind: 'message',
  space_id: 'chat',
  ref_id: `message-${id}`,
  title: 'Match',
  snippet,
});
const first = () => ({
  hits: Array.from({ length: 30 }, (_, n) => hit(n)),
  next_cursor: 'opaque-generation-one',
});
const more = (f: Awaited<ReturnType<typeof fixture>>) =>
  f.list.ListFooterComponent.props.children.props;
async function close(f: Awaited<ReturnType<typeof fixture>>) {
  await f.root.close();
  await f.store.close();
}

for (const change of ['edit', 'delete'] as const)
  test(`actual canonical ${change} event replaces stale results in the compiled route without changing owner or query`, async () => {
    const f = await fixture('needle', 'account', first(), 'ios', true);
    try {
      f.searchRead(async () => ({
        hits: change === 'edit' ? [hit(0, 'edited canonical text')] : [],
        next_cursor: null,
      }));
      await f.root.act(() => f.store.refresh(['chat']));
      assert.equal(f.input.value, 'needle');
      assert.equal(f.queries.at(-1).cursor, null);
      assert.equal(
        f.list.data.some((row: any) => row.snippet === 'original text'),
        false,
      );
      assert.equal(f.list.data.length, change === 'edit' ? 1 : 0);
      if (change === 'edit') assert.equal(f.list.data[0].snippet, 'edited canonical text');
      assert.equal(f.list.ListFooterComponent, null);
    } finally {
      await close(f);
    }
  });

test('a clock-only canonical refresh requalifies the same first page and preserves all accepted pages and refocus retention', async () => {
  const f = await fixture('needle', 'account', first(), 'ios', true);
  try {
    f.searchRead(async (command) =>
      command.cursor
        ? { hits: Array.from({ length: 15 }, (_, n) => hit(n + 30)), next_cursor: null }
        : first(),
    );
    await f.root.act(() => more(f).onPress());
    assert.equal(f.list.data.length, 45);
    const accepted = f.list.data,
      fresh = deferred<any>();
    f.searchRead(() => fresh.promise);
    f.canonicalSnapshot.clock_ms = '1900000000000';
    await f.root.act(() => f.store.refresh());
    assert.equal(
      f.list.data,
      accepted,
      'Pending harmless refresh preserves the actual list data and scroll identity',
    );
    await f.root.act(async () => {
      fresh.resolve(first());
      await fresh.promise;
    });
    assert.equal(f.list.data, accepted, 'Equal canonical results keep the accepted pages intact');
    assert.equal(f.list.data.length, 45, 'A new snapshot object/clock is not an index mutation');
    assert.equal(f.queries.length, 3);
    await f.root.act(() => f.focus(false));
    await f.root.act(() => f.focus(true));
    assert.equal(f.input.value, 'needle');
    assert.equal(f.list.data.length, 45);
    assert.equal(f.queries.length, 3, 'Focus alone preserves accepted pages without another query');
  } finally {
    await close(f);
  }
});

test('a canonical mutation while blurred is requalified on return instead of retaining stale results', async () => {
  const f = await fixture('needle', 'account', first(), 'ios', true);
  try {
    await f.root.act(() => f.focus(false));
    f.searchRead(async () => ({ hits: [], next_cursor: null }));
    await f.root.act(() => f.store.refresh(['chat']));
    assert.equal(f.queries.length, 1, 'An unfocused route does not launch a new search');
    await f.root.act(() => f.focus(true));
    assert.equal(f.input.value, 'needle');
    assert.deepEqual(Array.from(f.list.data), []);
    assert.equal(f.queries.length, 2);
  } finally {
    await close(f);
  }
});

test('canonical requalification preserves the visible projection while fencing an old pending continuation', async () => {
  const f = await fixture('needle', 'account', first(), 'ios', true);
  try {
    const oldOpen = f.list.renderItem({ item: f.list.data[0] }).props.onPress;
    const oldPage = deferred<any>(),
      fresh = deferred<any>();
    f.searchRead((command) => (command.cursor ? oldPage.promise : fresh.promise));
    await f.root.act(() => more(f).onPress());
    await f.root.act(() => f.store.refresh(['chat']));
    assert.equal(
      f.list.data.length,
      30,
      'A read-only qualification preserves the visible projection without flashing',
    );
    await f.root.act(() => oldOpen());
    assert.equal(
      f.reads.length,
      0,
      'An action captured before canonical invalidation cannot revive the old result',
    );
    await f.root.act(async () => {
      fresh.resolve({ hits: [hit(55, 'current')], next_cursor: null });
      await fresh.promise;
    });
    await f.root.act(async () => {
      oldPage.resolve({ hits: [hit(99)], next_cursor: null });
      await oldPage.promise;
    });
    assert.deepEqual(
      Array.from(f.list.data, (row: any) => row.ref_id),
      ['message-55'],
    );
  } finally {
    await close(f);
  }
});

test('failed canonical qualification removes stale results and exposes explicit retry', async () => {
  const f = await fixture('needle', 'account', first(), 'ios', true);
  try {
    f.searchRead(async () => {
      throw Error('Core temporarily unavailable');
    });
    await f.root.act(() => f.store.refresh(['chat']));
    assert.equal(f.list.data.length, 0);
    assert.ok(f.list.ListHeaderComponent);
    f.searchRead(async () => ({ hits: [hit(50, 'fresh retry')], next_cursor: null }));
    await f.root.act(() => f.list.ListHeaderComponent.props.children[1].props.onPress());
    assert.equal(f.list.data[0].snippet, 'fresh retry');
    assert.equal(f.input.value, 'needle');
  } finally {
    await close(f);
  }
});

test('a successful canonical read restores results after an earlier qualification failure', async () => {
  const f = await fixture('needle', 'account', first(), 'ios', true);
  try {
    f.searchRead(async () => {
      throw Error('Temporary connection failure');
    });
    await f.root.act(() => f.store.refresh());
    assert.equal(f.list.data.length, 0);
    f.searchRead(async () => first());
    await f.root.act(() => f.store.refresh());
    assert.equal(f.list.data.length, 30);
    assert.equal(f.list.ListHeaderComponent, null);
  } finally {
    await close(f);
  }
});
