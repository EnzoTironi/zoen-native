import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TimelineEntry } from './types';
import {
  messageCanEdit,
  messageCanInteract,
  messageCard,
  messagesAreGrouped,
  sameMessageDay,
  largeEmojiFontSize,
} from './message-presentation.ts';

function entry(patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: 'message',
    seq: '1',
    at_ms: '1000',
    author: { id: 'me', name: 'Me', is_me: true } as TimelineEntry['author'],
    kind: { Message: { text: 'A real message', card: null } },
    delivery: 'Sent',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
    ...patch,
  };
}

test('bubble continuity ends at a time gap, author change, reversed clock, or system event', () => {
  const previous = entry();
  assert.equal(messagesAreGrouped(undefined, previous), false);
  assert.equal(messagesAreGrouped(previous, entry({ at_ms: '600999' })), true);
  assert.equal(messagesAreGrouped(previous, entry({ at_ms: '601000' })), false);
  assert.equal(messagesAreGrouped(previous, entry({ at_ms: '999' })), false);
  assert.equal(messagesAreGrouped(previous, entry({ at_ms: 'NaN' })), false);
  assert.equal(
    messagesAreGrouped(previous, entry({ author: { ...previous.author, id: 'other' } })),
    false,
  );
  assert.equal(
    messagesAreGrouped(entry({ kind: { System: { text: 'Joined' } } }), previous),
    false,
  );
});

test('deleted message never exposes its retained media/card or mutation actions', () => {
  const attached = entry({
    deleted: true,
    kind: {
      Message: {
        text: 'Private',
        card: { item_id: 'secret' } as NonNullable<ReturnType<typeof messageCard>>,
      },
    },
  });
  assert.equal(messageCard(attached), null);
  assert.equal(messageCanEdit(attached), false);
  assert.equal(messageCanInteract(attached), false);
});

test('text and file-caption editing is author-only, excludes voice/miniapps and log activity', () => {
  assert.equal(messageCanEdit(entry()), true);
  assert.equal(messageCanEdit(entry({ author: { ...entry().author, is_me: false } })), false);
  assert.equal(
    messageCanEdit(entry({ voice: { item_id: 'voice' } as TimelineEntry['voice'] })),
    false,
  );
  const file = entry({
    kind: {
      Message: {
        text: 'Caption',
        card: { item_id: 'photo', kind_id: 'file' } as NonNullable<ReturnType<typeof messageCard>>,
      },
    },
  });
  assert.ok('Message' in file.kind);
  assert.equal(messageCanEdit(file), true);
  assert.equal(
    messageCanEdit({ ...file, kind: { Message: { ...file.kind.Message!, text: '' } } }),
    true,
  );
  assert.equal(messageCanEdit({ ...file, author: { ...file.author, is_me: false } }), false);
  assert.equal(messageCanEdit({ ...file, deleted: true }), false);
  assert.equal(
    messageCanEdit(
      entry({
        kind: {
          Message: {
            text: 'Shared',
            card: { item_id: 'card' } as NonNullable<ReturnType<typeof messageCard>>,
          },
        },
      }),
    ),
    false,
  );
  const activity = entry({
    kind: {
      ItemEdited: { item_id: 'page', title: 'Page', version: 2, note: 'Edited', is_undo: false },
    },
  });
  assert.equal(messageCanEdit(activity), false);
  assert.equal(messageCanInteract(activity), false);
});

test('bubble fingerprint preserves direction, thread, forward attribution and civil-date boundaries', () => {
  const prior = entry();
  for (const changed of [
    entry({ author: { ...prior.author, is_me: false } }),
    entry({ in_thread: 'different-root' }),
    entry({ forwarded: true }),
    entry({ deleted: true }),
  ])
    assert.equal(messagesAreGrouped(prior, changed), false);
  assert.equal(
    messagesAreGrouped(entry({ forwarded: true }), entry({ forwarded: true })),
    false,
    'the DTO cannot prove equal original forward authors',
  );
  const before = entry({ at_ms: String(new Date(2026, 9, 20, 23, 59).getTime()) });
  const after = entry({ at_ms: String(new Date(2026, 9, 21, 0, 1).getTime()) });
  assert.equal(sameMessageDay(before, after), false);
  assert.equal(messagesAreGrouped(before, after), false);
  assert.equal(sameMessageDay(undefined, after), false);
  assert.equal(sameMessageDay(entry({ at_ms: 'broken' }), after), false);
});

test('static large emoji preserves ZWJ/skin-tone/flag/keycap graphemes and rejects text lookalikes', () => {
  for (const single of ['👨‍👩‍👧‍👦', '👍🏽', '🇧🇷', '1️⃣', '☀️'])
    assert.equal(largeEmojiFontSize(single, true), 94);
  assert.equal(largeEmojiFontSize('👋💚', true), 78);
  assert.equal(largeEmojiFontSize('👋 💚 🙂', true), 64);
  assert.equal(largeEmojiFontSize('👋\n💚', true), 78);
  for (const text of ['123', 'hello 🙂', '#', '*', '', '  ', '💚'.repeat(10)])
    assert.equal(largeEmojiFontSize(text, true), null);
  assert.equal(largeEmojiFontSize('💚', false), null);
});
