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
