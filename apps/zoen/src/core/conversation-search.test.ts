import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ConversationSearch, conversationQueryFits } from './conversation-search.ts';
import type { UniversalHit } from './generated/UniversalHit';
import type { MessageSearchPage } from './generated/MessageSearchPage';

const hit = (id: string) => ({ kind: 'message', ref_id: id, space_id: 'space' }) as UniversalHit;
function deferred<T>() {
  let resolve!: (result: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('old query and closed search cannot publish a late canonical response', async () => {
  const old = deferred<MessageSearchPage>();
  const search = new ConversationSearch({
    current: () => true,
    search: async (query) =>
      query === 'old' ? old.promise : { hits: [hit('new')], next_cursor: null },
  });
  search.open();
  search.setQuery('old');
  const pending = search.search();
  search.setQuery('new');
  await search.search();
  old.resolve({ hits: [hit('old')], next_cursor: 'stale' });
  await pending;
  assert.equal(search.getSnapshot().hits[0].ref_id, 'new');
  const late = deferred<MessageSearchPage>();
  const closed = new ConversationSearch({ current: () => true, search: () => late.promise });
  closed.open();
  closed.setQuery('term');
  const reading = closed.search();
  closed.close();
  late.resolve({ hits: [hit('late')], next_cursor: null });
  await reading;
  assert.equal(closed.getSnapshot().active, false);
  assert.deepEqual(closed.getSnapshot().hits, []);
});

test('device/session scope changes fence first-page and page-boundary results', async () => {
  let current = true;
  const late = deferred<MessageSearchPage>();
  let calls = 0;
  const search = new ConversationSearch({
    current: () => current,
    search: async () => (++calls === 1 ? { hits: [hit('a')], next_cursor: 'next' } : late.promise),
  });
  search.open();
  search.setQuery('term');
  await search.search();
  const page = search.move('older');
  current = false;
  late.resolve({ hits: [hit('private')], next_cursor: null });
  await page;
  assert.deepEqual(
    search.getSnapshot().hits.map((h) => h.ref_id),
    ['a'],
  );
  await search.move('newer');
  assert.equal(search.getSnapshot().index, 0);
});

test('older/newer navigation pages real IDs, deduplicates boundary overlap and never wraps', async () => {
  const requests: (string | null)[] = [];
  const search = new ConversationSearch({
    current: () => true,
    search: async (query, cursor) => {
      assert.equal(query, 'find');
      requests.push(cursor);
      return cursor
        ? { hits: [hit('b'), hit('c')], next_cursor: null }
        : { hits: [hit('a'), hit('b')], next_cursor: 'next' };
    },
  });
  search.open();
  search.setQuery(' find ');
  await search.search();
  await search.move('newer');
  assert.equal(search.getSnapshot().index, 0);
  await search.move('older');
  assert.equal(search.getSnapshot().hits[search.getSnapshot().index].ref_id, 'b');
  await search.move('older');
  assert.equal(search.getSnapshot().hits[search.getSnapshot().index].ref_id, 'c');
  await search.move('older');
  assert.equal(search.getSnapshot().index, 2);
  await search.move('newer');
  assert.equal(search.getSnapshot().index, 1);
  assert.deepEqual(requests, [null, 'next']);
  assert.equal(search.getSnapshot().cursor, null);
});

test('stale cursor fails explicitly; retry starts a new canonical search instead of skipping history', async () => {
  let mutate = false;
  const requests: (string | null)[] = [];
  const search = new ConversationSearch({
    current: () => true,
    search: async (_, cursor) => {
      requests.push(cursor);
      if (cursor) throw new Error('stale canonical cursor');
      return { hits: [hit(mutate ? 'updated' : 'a')], next_cursor: mutate ? null : 'next' };
    },
  });
  search.open();
  search.setQuery('term');
  await search.search();
  await search.move('older');
  assert.equal(search.getSnapshot().completed, false);
  assert.match(search.getSnapshot().error!, /stale/);
  mutate = true;
  await search.search();
  assert.equal(search.getSnapshot().hits[0].ref_id, 'updated');
  assert.deepEqual(requests, [null, 'next', null]);
});

test('only fresh successful empty searches complete; edits/reads/errors cannot advertise no-results AI', async () => {
  let fail = false;
  const search = new ConversationSearch({
    current: () => true,
    search: async () => {
      if (fail) throw new Error('offline');
      return { hits: [], next_cursor: null };
    },
  });
  search.open();
  await search.search();
  assert.equal(search.getSnapshot().completed, false);
  search.setQuery('term');
  await search.search();
  assert.equal(search.getSnapshot().completed, true);
  search.setQuery('other');
  assert.equal(search.getSnapshot().completed, false);
  fail = true;
  await search.search();
  assert.equal(search.getSnapshot().completed, false);
});

test('UTF-8 limit handles emoji/accents without truncating the canonical query', async () => {
  assert.equal(conversationQueryFits('a'.repeat(512)), true);
  assert.equal(conversationQueryFits('á'.repeat(256)), true);
  assert.equal(conversationQueryFits('😀'.repeat(128)), true);
  assert.equal(conversationQueryFits('😀'.repeat(129)), false);
  assert.equal(conversationQueryFits('a'.repeat(513)), false);
  let calls = 0;
  const search = new ConversationSearch({
    current: () => true,
    search: async () => {
      calls++;
      return { hits: [], next_cursor: null };
    },
  });
  search.open();
  search.setQuery('😀'.repeat(129));
  await search.search();
  assert.equal(calls, 0);
});

test('returning to search retries an interrupted first page and discards its late result', async () => {
  const interrupted = deferred<MessageSearchPage>();
  const resumed = deferred<MessageSearchPage>();
  let calls = 0;
  const search = new ConversationSearch({
    current: () => true,
    search: () => (++calls === 1 ? interrupted.promise : resumed.promise),
  });
  search.open();
  search.setQuery('term');
  const pending = search.search();
  search.suspend();
  search.open();
  assert.equal(calls, 2);
  resumed.resolve({ hits: [hit('fresh')], next_cursor: null });
  interrupted.resolve({ hits: [hit('stale')], next_cursor: null });
  await pending;
  assert.deepEqual(search.getSnapshot().hits, [hit('fresh')]);
  assert.equal(search.getSnapshot().loading, false);
});

test('a successful pagination retry clears a temporary search failure', async () => {
  let failNextPage = true;
  const search = new ConversationSearch({
    current: () => true,
    search: async (_, cursor) => {
      if (!cursor) return { hits: [hit('first')], next_cursor: 'older' };
      if (failNextPage) throw new Error('temporarily unavailable');
      return { hits: [hit('second')], next_cursor: null };
    },
  });
  search.open();
  search.setQuery('term');
  await search.search();
  await search.move('older');
  assert.equal(search.getSnapshot().completed, false);
  assert.equal(search.getSnapshot().index, 0);

  failNextPage = false;
  await search.move('older');
  const result = search.getSnapshot();
  assert.equal(result.completed, true);
  assert.equal(result.error, null);
  assert.equal(result.index, 1);
  assert.deepEqual(result.hits, [hit('first'), hit('second')]);
});

test('canonical refresh keeps the selected result while it remains on the first page', async () => {
  let hits = [hit('a'), hit('b')];
  const search = new ConversationSearch({
    current: () => true,
    search: async () => ({ hits, next_cursor: null }),
  });
  search.open();
  search.setQuery('term');
  await search.search();
  await search.move('older');

  hits = [hit('new'), hit('a'), hit('b')];
  await search.refreshCanonical();
  assert.equal(search.getSnapshot().index, 2);

  hits = [hit('new'), hit('a')];
  await search.refreshCanonical();
  assert.equal(search.getSnapshot().index, 0);
});

test('conversation updates do not submit a draft search query', async () => {
  const queries: string[] = [];
  const search = new ConversationSearch({
    current: () => true,
    search: async (query) => {
      queries.push(query);
      return { hits: [], next_cursor: null };
    },
  });
  search.open();
  search.setQuery('submitted');
  await search.search();
  search.setQuery('draft');
  await search.refreshCanonical();
  assert.deepEqual(queries, ['submitted']);
  assert.equal(search.getSnapshot().completed, false);
});

test('assistant action only accepts the empty result snapshot presented to the user', async () => {
  let authorized = true;
  const search = new ConversationSearch({
    current: () => authorized,
    search: async () => ({ hits: [], next_cursor: null }),
  });
  search.open();
  search.setQuery('first');
  await search.search();
  const first = search.getSnapshot();
  assert.equal(search.assistantQuery(first), 'first');

  search.setQuery('second');
  await search.search();
  const second = search.getSnapshot();
  assert.equal(search.assistantQuery(first), null);
  assert.equal(search.assistantQuery(second), 'second');
  authorized = false;
  assert.equal(search.assistantQuery(second), null);
});

test('a conversation update while suspended requalifies results before offering AI again', async () => {
  const refreshed = deferred<MessageSearchPage>();
  let calls = 0;
  const search = new ConversationSearch({
    current: () => true,
    search: async () =>
      ++calls === 1 ? { hits: [], next_cursor: null } : refreshed.promise,
  });
  search.open();
  search.setQuery('term');
  await search.search();
  const empty = search.getSnapshot();
  assert.equal(search.assistantQuery(empty), 'term');

  search.suspend();
  await search.refreshCanonical();
  assert.equal(calls, 1);
  search.open();
  assert.equal(calls, 2);
  assert.equal(search.assistantQuery(search.getSnapshot()), null);

  refreshed.resolve({ hits: [hit('new')], next_cursor: null });
  await refreshed.promise;
  await Promise.resolve();
  assert.deepEqual(search.getSnapshot().hits, [hit('new')]);
  assert.equal(search.assistantQuery(empty), null);
  assert.equal(search.assistantQuery(search.getSnapshot()), null);
});
