import assert from 'node:assert/strict';
import test from 'node:test';
import { mediaAlbums } from '../media/media-groups.ts';
import type { TimelineEntry } from './types';

function member(index: number, overrides: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: `message-${index}`,
    author: { id: 'alice' },
    at_ms: '42',
    kind: { Message: { text: '', card: { kind_id: 'file' } } },
    media: { presentation: 'media', group: { id: `album_${'a'.repeat(64)}`, index, count: 2 } },
    in_thread: null,
    reply_to: null,
    ...overrides,
  } as TimelineEntry;
}

test('explicit albums keep canonical IDs and signed order despite arrival order or timestamps', () => {
  const entries = [member(1), member(0, { at_ms: '999999' })];
  const [album] = mediaAlbums('room', entries);
  assert.deepEqual(
    album?.entries.map((entry) => entry.id),
    ['message-0', 'message-1'],
  );
  assert.equal(album?.entries[1], entries[0]);
  assert.notEqual(mediaAlbums('other', entries)[0]?.id, album?.id);
});

test('partial albums, duplicate indices, foreign authors, destinations and tombstones stay separate', () => {
  const a = member(0);
  const b = member(1);
  for (const entries of [
    [a],
    [a, a],
    [a, member(0, { id: 'duplicate-index' })],
    [a, member(1, { author: { id: 'bob' } as TimelineEntry['author'] })],
    [a, member(1, { in_thread: 'other-thread' })],
    [a, member(1, { deleted: true })],
    [a, { ...b, media: { ...b.media!, presentation: 'file' as const } }],
  ])
    assert.deepEqual(mediaAlbums('room', entries), []);
  assert.deepEqual(
    mediaAlbums('room', [
      { ...a, media: undefined },
      { ...b, media: undefined },
    ]),
    [],
  );
});
