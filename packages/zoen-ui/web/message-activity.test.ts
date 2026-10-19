import test from 'node:test';
import assert from 'node:assert/strict';
import { parseConversation } from './messages.ts';

const author = '11'.repeat(32);
const activity = {
  space_id: 'marina',
  item_id: 'drawing',
  title: 'Adventure',
  version: 2,
  note: 'Updated the drawing',
  is_undo: false,
};
const event = {
  id: 'canonical-client-id',
  entry: 'ab'.repeat(32),
  seq: '18446744073709551615',
  author,
  name: 'Enzo',
  text: activity.note,
  at_ms: 1234,
  delivery: 'confirmed',
  failure: null,
  item_edit: activity,
};
const conversation = {
  id: 'marina',
  title: 'Marina',
  kind: 'direct',
  privacy: 'EndToEnd',
  roster: { [author]: 'Owner' },
  profiles: [],
  prefs: { pinned: false, archived: false, muted_until_ms: null },
  event_count: '18446744073709551615',
  can_send: true,
  waiting: null,
  unreadable: 0,
  messages: [event],
};

test('browser boundary retains typed activity, stable IDs and exact u64 sequence without text inference', () => {
  const parsed = parseConversation(conversation).messages[0]!;
  assert.deepEqual(parsed.item_edit, activity);
  assert.equal(parsed.id, event.id);
  assert.equal(parsed.entry, event.entry);
  assert.equal(parsed.seq, event.seq);
  const human = parseConversation({ ...conversation, messages: [{ ...event, item_edit: null }] })
    .messages[0]!;
  assert.equal(human.item_edit, null);
  assert.equal(human.text, parsed.text);
  const undo = parseConversation({
    ...conversation,
    messages: [
      {
        ...event,
        text: 'Restored v1',
        item_edit: { ...activity, note: 'Restored v1', version: 3, is_undo: true },
      },
    ],
  }).messages[0]!;
  assert.equal(undo.item_edit?.version, 3);
  assert.equal(undo.item_edit?.is_undo, true);
});

test('browser boundary rejects activity from another conversation, invalid versions and mixed message authority', () => {
  for (const edit of [
    { ...activity, space_id: 'elsewhere' },
    { ...activity, item_id: '' },
    { ...activity, version: 1 },
    { ...activity, version: 4294967296 },
    { ...activity, version: 2.5 },
    { ...activity, is_undo: 'false' },
  ])
    assert.throws(() =>
      parseConversation({ ...conversation, messages: [{ ...event, item_edit: edit }] }),
    );
  for (const patch of [
    { text: 'Different text' },
    { card: {} },
    { file: {} },
    { quote: {} },
    { thread: 'root' },
    { deleted: true },
    { edited: true },
    { forwarded: true },
    { reactions: [{}] },
  ])
    assert.throws(
      () => parseConversation({ ...conversation, messages: [{ ...event, ...patch }] }),
      /activity/,
    );
});

test('a protocol-valid signed human thread does not invalidate its typed activity root', () => {
  const root = {
    ...event,
    seq: '9007199254740992',
    replies: 1,
    repliers: ['Other device'],
    last_reply_ms: 1240,
  };
  const reply = {
    ...event,
    id: 'reply-client-id',
    entry: 'cd'.repeat(32),
    seq: '9007199254740993',
    text: 'Please keep the blue arrow.',
    item_edit: null,
    thread: event.entry,
    at_ms: 1240,
  };
  const parsed = parseConversation({ ...conversation, messages: [root, reply] });
  assert.equal(parsed.messages[0]?.replies, 1);
  assert.deepEqual(parsed.messages[0]?.item_edit, activity);
  assert.equal(parsed.messages[1]?.item_edit, null);
  assert.equal(parsed.messages[1]?.thread, root.entry);
  assert.equal(parsed.messages[1]?.text, reply.text);
});

test('a signed document projection diagnostic is explicit while human source IDs survive parsing', () => {
  const parsed = parseConversation({
    ...conversation,
    integrity_error: 'Two signed spaces claim the same item',
    messages: [{ ...event, item_edit: null }],
  });
  assert.equal(parsed.integrity_error, 'Two signed spaces claim the same item');
  assert.equal(parsed.messages[0]?.id, event.id);
  assert.equal(parsed.messages[0]?.entry, event.entry);
  assert.equal(parseConversation(conversation).integrity_error, null);
});
