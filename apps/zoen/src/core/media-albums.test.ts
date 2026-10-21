import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TimelineEntry } from './types';
import { albumDelivery, projectMediaAlbums } from './media-albums.ts';
import { projectConversationTimeline } from './conversation-timeline.ts';

const groupId = `album_${'ab'.repeat(32)}`;
function media(index: number, patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: `message-${index}`,
    seq: String(index + 1),
    at_ms: String(index * 1000),
    author: { id: 'author', is_me: true, name: 'Author' } as TimelineEntry['author'],
    kind: {
      Message: {
        text: `Caption ${index}`,
        card: {
          item_id: `file-${index}`,
          title: `Photo ${index}`,
          summary: '',
          kind_label: 'File',
          version: 1,
          kind_id: 'file',
          app: null,
          total_cents: null,
          budget_cents: null,
          line_count: 0,
          done_count: 0,
        },
      },
    },
    media: { presentation: 'media', group: { id: groupId, count: 3, index } },
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
const complete = () => [media(0), media(1), media(2)];

test('complete signed albums preserve actual child order, captions, identities and unrelated interleaved rows', () => {
  const values = complete();
  const other = media(7, { id: 'ordinary', media: undefined });
  const entries = [values[2]!, other, values[0]!, values[1]!];
  const before = JSON.stringify(entries);
  const rows = projectConversationTimeline(entries, [], 'qa', null, true);
  assert.equal(rows.length, 2);
  assert.strictEqual(rows[0]!.entry, values[2]);
  assert.deepEqual(rows[0]!.album!.entries, values);
  assert.strictEqual(rows[1]!.entry, other);
  assert.equal(JSON.stringify(entries), before);
  assert.equal(rows[0]!.album!.spaceId, 'qa');
});

test('partial, conflicting, deleted, duplicate and cross-author membership never hides a message', () => {
  for (const entries of [
    complete().slice(1),
    [media(0), media(1), media(1, { id: 'duplicate-index' })],
    [media(0), media(1), media(2, { deleted: true })],
    [media(0), media(1), media(2, { in_thread: 'other-thread' })],
    [
      media(0),
      media(1),
      media(2, {
        reply_to: {
          id: 'other',
          text: '',
          author: { id: 'x', name: 'X' },
        } as TimelineEntry['reply_to'],
      }),
    ],
    [media(0), media(1), media(2, { author: { id: 'other' } as TimelineEntry['author'] })],
    [
      media(0),
      media(1),
      media(2, { media: { presentation: 'media', group: { id: groupId, count: 4, index: 2 } } }),
    ],
    [media(0), media(1), media(2, { forwarded: true })],
  ]) {
    const result = projectMediaAlbums(entries, 'qa');
    assert.equal(result.albums.size, 0);
    assert.equal(result.hidden.size, 0);
  }
});

test('ordinary adjacent photos never become an album and explicit file presentation stays a file', () => {
  for (const entries of [
    complete().map((entry) => ({ ...entry, media: undefined })),
    complete().map((entry) => ({
      ...entry,
      media: { ...entry.media!, presentation: 'file' as const },
    })),
  ]) {
    assert.equal(projectMediaAlbums(entries, 'qa').albums.size, 0);
  }
});

test('reveals and per-message reaction/thread affordances keep canonical rows individually addressable', () => {
  assert.equal(projectMediaAlbums(complete(), 'qa', 'message-1').hidden.size, 0);
  assert.equal(
    projectMediaAlbums([media(0), media(1), media(2, { thread_replies: 1 })], 'qa').hidden.size,
    0,
  );
  const a = projectMediaAlbums(complete(), 'qa').albums.values().next().value!;
  const b = projectMediaAlbums(complete(), 'other').albums.values().next().value!;
  assert.notEqual(a.id, b.id);
});

test('status cannot claim an album is delivered or read before every child has reached it', () => {
  assert.equal(
    albumDelivery(complete().map((entry) => ({ ...entry, delivery: 'Local' }))),
    'Local',
  );
  for (const status of ['Failed', 'Sending', 'Sent', 'Delivered'] as const) {
    assert.equal(
      albumDelivery([media(0, { delivery: 'Read' }), media(1, { delivery: status })]),
      status,
    );
  }
  assert.equal(albumDelivery(complete().map((entry) => ({ ...entry, delivery: 'Read' }))), 'Read');
});
