import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';
const pause = () => new Promise((resolve) => setTimeout(resolve, 240));

test('compiled Links choice browses without text and keeps dates, author, conversation and message navigation', async () => {
  const f = await fixture(
    '',
    'account',
    {
      hits: [
        {
          kind: 'message',
          ref_id: 'linked',
          space_id: 'chat',
          title: 'Alice',
          snippet: 'Release',
          at_ms: 100,
        },
      ],
      next_cursor: null,
    },
    'ios',
    false,
    { space: 'chat' },
  );
  try {
    await f.root.act(() => f.identities.onChange({ space_id: 'chat', author_id: 'author' }));
    const period = { start_ms: 10, end_ms: 200 };
    await f.root.act(() => f.period.onChange(period));
    await f.root.act(() => f.filters.get('chat.searchLinks')!());
    await f.root.act(pause);
    const query = f.queries.at(-1);
    assert.equal(query.query, '');
    assert.deepEqual(JSON.parse(JSON.stringify(query.kinds)), ['link']);
    assert.deepEqual(JSON.parse(JSON.stringify(query.period)), period);
    assert.deepEqual(JSON.parse(JSON.stringify(query.messages)), {
      space_id: 'chat',
      author_id: 'author',
    });
    assert.equal(query.cursor, null);
    assert.equal(f.list.data[0].ref_id, 'linked');
    await f.root.act(() => f.list.renderItem({ item: f.list.data[0] }).props.onPress());
    await f.root.act(async () => {
      f.pending.resolve({ id: 'linked', deleted: false, in_thread: 'root' });
      await f.pending.promise;
    });
    assert.deepEqual(JSON.parse(JSON.stringify(f.routes)), [
      {
        pathname: '/thread/[space]/[root]',
        params: { space: 'chat', root: 'root', jump: 'linked' },
      },
    ]);
    assert.equal(f.dismissals, 1);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});
