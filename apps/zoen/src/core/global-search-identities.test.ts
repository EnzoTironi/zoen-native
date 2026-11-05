import assert from 'node:assert/strict';
import test from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';
import { deferred } from './store.fixtures.ts';
const tick = () => new Promise((resolve) => setTimeout(resolve, 220));
const first = {
  hits: [{ kind: 'message', space_id: 'chat', ref_id: 'one', title: 'Match', snippet: 'needle' }],
  next_cursor: 'cursor-one',
};

test('compiled route combines exact author/chat identities with media and period and resets pagination', async () => {
  const f = await fixture('needle', 'account', first);
  try {
    const old = f.identities;
    await f.root.act(() => f.identities.onChange({ author_id: 'alice', space_id: 'chat' }));
    await f.root.act(tick);
    assert.deepEqual(JSON.parse(JSON.stringify(f.queries.at(-1).messages)), {
      author_id: 'alice',
      space_id: 'chat',
    });
    assert.equal(f.queries.at(-1).cursor, null);
    assert.equal(f.input.value, 'needle');
    await f.root.act(() => f.filters.get('chat.searchMediaFilter')!());
    await f.root.act(tick);
    assert.deepEqual(Array.from(f.queries.at(-1).kinds), ['media']);
    assert.equal(f.queries.at(-1).messages.author_id, 'alice');
    await f.root.act(() => f.filters.get('chat.searchLastWeek')!());
    await f.root.act(tick);
    assert.ok(f.queries.at(-1).period);
    assert.equal(f.queries.at(-1).messages.space_id, 'chat');
    const count = f.queries.length;
    await f.root.act(() => old.onChange({ author_id: 'stale' }));
    await f.root.act(tick);
    assert.equal(f.queries.length, count);
    await f.root.act(() => f.filters.get('chat.people')!());
    await f.root.act(tick);
    assert.equal(f.queries.at(-1).messages, undefined);
    assert.deepEqual(Array.from(f.queries.at(-1).kinds), ['person', 'agent']);
  } finally {
    await f.root.close();
  }
});

test('a late old page cannot replace a changed identity filter', async () => {
  const f = await fixture('needle', 'account', first);
  try {
    const pending = deferred<any>();
    f.searchRead(async (command) =>
      command.cursor ? pending.promise : { hits: [], next_cursor: null },
    );
    await f.root.act(() => f.list.ListFooterComponent.props.children.props.onPress());
    await f.root.act(() => f.identities.onChange({ author_id: 'alice' }));
    await f.root.act(tick);
    await f.root.act(async () => {
      pending.resolve({ hits: [{ ...first.hits[0], ref_id: 'stale-page' }], next_cursor: null });
      await pending.promise;
    });
    assert.equal(f.input.value, 'needle');
    assert.deepEqual(Array.from(f.list.data), []);
    assert.equal(f.queries.at(-1).messages.author_id, 'alice');
  } finally {
    await f.root.close();
  }
});

test('an exact author selection can browse without text and clearing it restores the empty query state', async () => {
  const f = await fixture('', 'account', { hits: [], next_cursor: null });
  try {
    await f.root.act(() => f.identities.onChange({ author_id: 'alice' }));
    await f.root.act(tick);
    assert.equal(f.queries.length, 1);
    assert.equal(f.queries[0].query, '');
    await f.root.act(() => f.identities.onChange(undefined));
    await f.root.act(tick);
    assert.equal(f.queries.length, 1);
  } finally {
    await f.root.close();
  }
});

test('a retained identity selector cannot restore the previous text query', async () => {
  const f = await fixture('old query', 'account', first);
  try {
    const old = f.identities;
    await f.root.act(() => f.input.onChangeText('new query'));
    await f.root.act(() => old.onChange({ author_id: 'old-author' }));
    await f.root.act(tick);
    assert.equal(f.input.value, 'new query');
    assert.equal(f.queries.at(-1).query, 'new query');
    assert.equal(f.queries.at(-1).messages, undefined);
  } finally {
    await f.root.close();
  }
});

test('an applied historical author keeps its name when another filter leaves no results', async () => {
  const f = await fixture('needle', 'account', {
    hits: [{ ...first.hits[0], persona: { id: 'former', name: 'Former member' } }],
    next_cursor: null,
  });
  try {
    await f.root.act(() => f.identities.onChange({ author_id: 'former' }));
    await f.root.act(tick);
    f.searchRead(async () => ({ hits: [], next_cursor: null }));
    await f.root.act(() => f.filters.get('chat.searchLastWeek')!());
    await f.root.act(tick);
    assert.equal(f.identities.labels.author, 'Former member');
    assert.equal(f.identities.value.author_id, 'former');
    assert.equal(
      f.identities.options.authors.some((item: any) => item.id === 'former'),
      false,
    );
  } finally {
    await f.root.close();
  }
});
