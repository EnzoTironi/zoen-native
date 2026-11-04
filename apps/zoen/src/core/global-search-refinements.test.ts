import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture } from './global-search-screen.fixtures.ts';
const pause = () => new Promise((resolve) => setTimeout(resolve, 240));

test('compiled media choices request the canonical category without dropping query, author, chat or custom dates', async () => {
  const f = await fixture('needle', 'account', { hits: [], next_cursor: null });
  try {
    await f.root.act(() => f.identities.onChange({ space_id: 'chat', author_id: 'author' }));
    await f.root.act(pause);
    const range = {
      start_ms: new Date(2026, 0, 1).getTime(),
      end_ms: new Date(2026, 1, 1).getTime(),
    };
    await f.root.act(() => f.period.onChange(range));
    await f.root.act(pause);
    await f.root.act(() => f.filters.get('chat.searchMediaFilter')!());
    await f.root.act(pause);
    for (const [label, kind] of [
      ['chat.searchPhotos', 'photo'],
      ['chat.searchVideos', 'video'],
      ['chat.searchAudio', 'audio'],
      ['chat.searchVoice', 'voice'],
      ['chat.searchFiles', 'file'],
    ]) {
      const old = f.queries.length;
      await f.root.act(() => f.filters.get(label)!());
      await f.root.act(pause);
      const query = f.queries.at(-1);
      assert.ok(f.queries.length > old);
      assert.deepEqual(JSON.parse(JSON.stringify(query.kinds)), [`media:${kind}`]);
      assert.equal(query.query, 'needle');
      assert.equal(query.cursor, null);
      assert.deepEqual(JSON.parse(JSON.stringify(query.period)), range);
      assert.deepEqual(JSON.parse(JSON.stringify(query.messages)), {
        space_id: 'chat',
        author_id: 'author',
      });
    }
    const retained = f.period;
    await f.root.act(() => f.focus(false));
    const count = f.queries.length;
    await f.root.act(() => retained.onChange({ start_ms: 1, end_ms: 2 }));
    await f.root.act(pause);
    assert.equal(f.queries.length, count);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});

test('scoped native search offers media categories and calendar filters; people are independent of message dates', async () => {
  const scoped = await fixture('', 'account', { hits: [], next_cursor: null }, 'ios', false, {
    space: 'chat',
  });
  try {
    assert.equal(scoped.filters.has('chat.people'), false);
    await scoped.root.act(() => scoped.filters.get('chat.searchMediaFilter')!());
    await scoped.root.act(pause);
    await scoped.root.act(() => scoped.filters.get('chat.searchVoice')!());
    await scoped.root.act(pause);
    const query = scoped.queries.at(-1);
    assert.deepEqual(JSON.parse(JSON.stringify(query.kinds)), ['media:voice']);
    assert.equal(query.query, '');
    assert.equal(query.messages.space_id, 'chat');
  } finally {
    await scoped.root.close();
  }
  const people = await fixture('friend', 'account', { hits: [], next_cursor: null });
  try {
    await people.root.act(() => people.period.onChange({ start_ms: 1, end_ms: 2 }));
    await people.root.act(pause);
    await people.root.act(() => people.filters.get('chat.people')!());
    await people.root.act(pause);
    assert.equal(people.queries.at(-1).period, undefined);
    assert.deepEqual(people.root.errors, []);
  } finally {
    await people.root.close();
  }
});
