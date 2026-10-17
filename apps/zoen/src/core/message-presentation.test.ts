import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TimelineEntry } from './types';
import {
  messageCanEdit,
  messageCanInteract,
  messageCard,
  messagesAreGrouped,
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
  assert.equal(messagesAreGrouped(previous, entry({ at_ms: '180999' })), true);
  assert.equal(messagesAreGrouped(previous, entry({ at_ms: '181000' })), false);
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

test('text editing is author-only, excludes voice/card messages, and excludes log activity', () => {
  assert.equal(messageCanEdit(entry()), true);
  assert.equal(messageCanEdit(entry({ author: { ...entry().author, is_me: false } })), false);
  assert.equal(
    messageCanEdit(entry({ voice: { item_id: 'voice' } as TimelineEntry['voice'] })),
    false,
  );
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
