import type { TimelineEntry } from './types';
import { mediaAlbums, type MediaAlbum as SignedMediaAlbum } from '../media/media-groups.ts';

export type MediaAlbum = SignedMediaAlbum &
  Readonly<{
    /** A visual projection of signed membership; every canonical message survives. */
    spaceId: string;
  }>;

/** Scope and ordered, complete signed membership are the only grouping inputs.
 * Time, file names, upload completion and adjacent same-author bubbles are not.
 * Incomplete or conflicting groups retain the ordinary per-message renderer. */
export function projectMediaAlbums(
  entries: readonly TimelineEntry[],
  spaceId: string,
  revealId?: string | null,
) {
  const albums = new Map<string, MediaAlbum>();
  const hidden = new Set<string>();
  for (const album of mediaAlbums(spaceId, entries)) {
    if (
      album.presentation !== 'media' ||
      album.entries.some(
        (entry) =>
          entry.id === revealId ||
          entry.forwarded ||
          entry.voice ||
          entry.reactions.length > 0 ||
          entry.thread_replies > 0,
      )
    )
      continue;
    const ids = new Set(album.entries.map((entry) => entry.id));
    const first = entries.find((entry) => ids.has(entry.id))!;
    // The first encountered message anchors the visual row. Child order is the
    // signed album order, even if independent transport completion interleaves.
    albums.set(first.id, { ...album, spaceId });
    for (const entry of album.entries) if (entry.id !== first.id) hidden.add(entry.id);
  }
  return { albums, hidden };
}

/** An album is fully delivered/read only when every canonical child is. */
export function albumDelivery(entries: readonly TimelineEntry[]): TimelineEntry['delivery'] {
  const order: TimelineEntry['delivery'][] = [
    'Failed',
    'Sending',
    'Local',
    'Sent',
    'Delivered',
    'Read',
  ];
  return order.find((status) => entries.some((entry) => entry.delivery === status)) ?? 'Sending';
}
