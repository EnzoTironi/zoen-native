import test from 'node:test';
import assert from 'node:assert/strict';
import { searchMessages } from './message-search.ts';
import type { Conversation, Message } from './messages.ts';

function message(id: string, text = 'café', at_ms = 1000): Message {
  return { id, entry: id, author: '11'.repeat(32), name: 'Ana', text, at_ms, delivery: 'confirmed', failure: null, ticks: 'sent', edited: false, deleted: false, forwarded: false, reactions: [], quote: null, thread: null, replies: 0, repliers: [], last_reply_ms: null };
}
function chat(id: string, messages: Message[]): Conversation { return { id, title: id, kind: 'group', can_send: true, waiting: null, unreadable: 0, unread: 0, mentions: 0, members: 2, messages }; }

test('message search scopes before limiting, normalizes Unicode and ignores deleted/pending messages', () => {
  const pending = { ...message('pending'), entry: null };
  const deleted = { ...message('deleted'), deleted: true };
  const reply = { ...message('reply'), thread: 'root' };
  const chats = [chat('other', [message('outside', 'café', 2000)]), chat('target', [message('a'), pending, deleted, reply])];
  assert.deepEqual(searchMessages(chats, ' CAFE\u0301 ', 'target', null, 1).hits.map((h) => h.message.id), ['a']);
  assert.deepEqual(searchMessages(chats, 'café', 'target').hits.map((h) => h.message.id), ['a', 'reply']);
  assert.equal(searchMessages(chats, '   ').hits.length, 0);
});

test('keyset pages have stable ties, bounded limits and no duplicate after newer inserts', () => {
  const chats = [chat('a', Array.from({ length: 105 }, (_, i) => message(String(i).padStart(3, '0'))))];
  assert.equal(searchMessages(chats, 'café', '', null, 500).hits.length, 100);
  const first = searchMessages(chats, 'café', '', null, 2);
  assert.ok(first.next);
  chats[0]!.messages.unshift(message('newer', 'café', 2000));
  const next = searchMessages(chats, 'café', '', first.next, 2);
  assert.deepEqual(next.hits.map((h) => h.message.id), ['002', '003']);
  const seen = new Set<string>(); let cursor = null;
  do {
    const page = searchMessages(chats, 'café', '', cursor, 7);
    for (const hit of page.hits) { assert.equal(seen.has(hit.message.id), false); seen.add(hit.message.id); }
    cursor = page.next;
  } while (cursor);
  assert.equal(seen.size, 106);
});

test('live edited/deleted projections immediately replace search text rather than indexing quotes', () => {
  const m = message('root', 'antiga');
  const quoted = { ...message('quote', 'resposta'), quote: { id: 'root', name: 'Ana', text: 'antiga' } };
  const chats = [chat('a', [m, quoted])];
  assert.equal(searchMessages(chats, 'antiga').hits.length, 1);
  m.text = 'nova'; m.edited = true;
  assert.equal(searchMessages(chats, 'antiga').hits.length, 0);
  assert.equal(searchMessages(chats, 'nova').hits.length, 1);
  m.deleted = true;
  assert.equal(searchMessages(chats, 'nova').hits.length, 0);
});
