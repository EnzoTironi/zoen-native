import test from 'node:test';
import assert from 'node:assert/strict';
import { parseConversation } from '../../../../packages/zoen-ui/web/messages.ts';
import { searchBrowser, searchBrowserMessages } from './browser-search.ts';
import type { Persona } from './generated/Persona.ts';

const identity = '11'.repeat(32);
const owner = '22'.repeat(32);
function person(id: string, name: string): Persona {
  return {
    id,
    name,
    handle: '',
    initials: '?',
    kind: 'Person',
    tint_hex: '#888888',
    glyph: null,
    bio: '',
    owner_id: null,
    owner_name: null,
    owner_tint_hex: null,
    photo_sha256: null,
    photo_ready: false,
    photo_mime: null,
    is_me: false,
    is_mine: false,
  };
}
function message(id: string, at_ms: number, text = 'café com Ana', extra = {}) {
  return {
    id,
    entry: id,
    seq: '18446744073709551615',
    author: identity,
    name: 'Ana',
    text,
    at_ms,
    delivery: 'confirmed',
    failure: null,
    ...extra,
  };
}
function chat(id: string, messages: unknown[]) {
  return parseConversation({
    id,
    title: `Viagem ${id}`,
    kind: 'group',
    privacy: 'EndToEnd',
    roster: { [identity]: 'Member' },
    profiles: [
      {
        id: identity,
        name: 'Horta',
        handle: 'garden',
        kind: 'Agent',
        owner,
        tint_hex: '#008800',
        glyph: 'leaf',
        bio: 'Cuida das plantas',
      },
    ],
    prefs: { pinned: false, archived: false, muted_until_ms: null },
    event_count: '100',
    can_send: true,
    waiting: null,
    unreadable: 0,
    messages,
  });
}

test('browser search pages stay scoped, include thread replies and refuse cursors from another query', () => {
  const conversations = [
    chat('a', [
      message('first', 30),
      message('thread', 20, 'café', { thread: 'root' }),
      message('deleted', 10, 'café', { deleted: true }),
      message('pending', 5, 'café', { entry: null }),
    ]),
    chat('b', [message('elsewhere', 40)]),
  ];
  const first = searchBrowserMessages(conversations, ' CAFE\u0301 ', 'a', null, 1, person);
  assert.deepEqual(
    first.hits.map((hit) => hit.ref_id),
    ['first'],
  );
  assert.ok(first.next_cursor);
  const next = searchBrowserMessages(conversations, 'café', 'a', first.next_cursor, 1, person);
  assert.deepEqual(
    next.hits.map((hit) => hit.ref_id),
    ['thread'],
  );
  assert.equal(next.next_cursor, null);
  assert.throws(
    () => searchBrowserMessages(conversations, 'tea', 'a', first.next_cursor, 1, person),
    /Reinicie/,
  );
  assert.throws(
    () => searchBrowserMessages(conversations, 'café', 'b', first.next_cursor, 1, person),
    /Reinicie/,
  );
  assert.throws(
    () => searchBrowserMessages(conversations, 'café', 'a', '{}', 1, person),
    /Reinicie/,
  );
});

test('universal browser search deduplicates actual profiles and honors kind filters and limits', () => {
  const conversations = [chat('a', [message('note', 1)]), chat('b', [])];
  const agent = searchBrowser(conversations, 'garden', ['agent'], 30, person);
  assert.equal(agent.hits.length, 1);
  assert.equal(agent.hits[0]?.ref_id, identity);
  assert.equal(agent.hits[0]?.kind, 'agent');
  assert.equal(searchBrowser(conversations, 'garden', ['person'], 30, person).hits.length, 0);
  assert.equal(searchBrowser(conversations, 'viagem', ['space'], 1, person).hits.length, 1);
  assert.equal(searchBrowser(conversations, ' ', [], 30, person).hits.length, 0);
  assert.match(agent.took_us, /^\d+$/);
});

test('message excerpts retain a distant match with its original exact message ID', () => {
  const conversations = [
    chat('a', [message('original-id', 1, `${'prefix '.repeat(100)}Café na praça`)]),
  ];
  const result = searchBrowser(conversations, 'cafe\u0301', ['message'], 10, person);
  assert.equal(result.hits[0]?.ref_id, 'original-id');
  assert.match(result.hits[0]?.snippet ?? '', /Café na praça/);
  assert.ok((result.hits[0]?.snippet.length ?? 0) < 225);
});
