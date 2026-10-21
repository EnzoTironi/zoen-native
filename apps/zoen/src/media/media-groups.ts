import type { TimelineEntry } from '../core/types';

export type MediaAlbum = Readonly<{
  id: string;
  presentation: 'media' | 'file';
  entries: readonly TimelineEntry[];
}>;

/** A partial/malformed album stays as ordinary individual messages. Consumers keep
 * every canonical entry id for reply, delete, search and receipt operations. */
export function mediaAlbums(
  spaceId: string,
  entries: readonly TimelineEntry[],
): readonly MediaAlbum[] {
  const candidates = new Map<string, TimelineEntry[]>();
  for (const entry of entries) {
    const group = entry.media?.group;
    if (!group) continue;
    const key = JSON.stringify([spaceId, entry.author.id, group.id]);
    const members = candidates.get(key) ?? [];
    members.push(entry);
    candidates.set(key, members);
  }
  const albums: MediaAlbum[] = [];
  for (const [id, members] of candidates) {
    const first = members[0]!;
    const count = first.media!.group!.count;
    if (!Number.isInteger(count) || count < 2 || count > 10 || members.length !== count) continue;
    const indices = new Set<number>();
    const ids = new Set<string>();
    if (
      members.some((entry) => {
        const group = entry.media!.group!;
        const valid =
          !entry.deleted &&
          'Message' in entry.kind &&
          entry.kind.Message.card?.kind_id === 'file' &&
          /^album_[0-9a-f]{64}$/.test(group.id) &&
          group.count === count &&
          Number.isInteger(group.index) &&
          group.index >= 0 &&
          group.index < count &&
          !indices.has(group.index) &&
          !ids.has(entry.id) &&
          entry.media!.presentation === first.media!.presentation &&
          entry.in_thread === first.in_thread &&
          entry.reply_to?.id === first.reply_to?.id;
        indices.add(group.index);
        ids.add(entry.id);
        return !valid;
      })
    )
      continue;
    albums.push({
      id,
      presentation: first.media!.presentation,
      entries: [...members].sort((a, b) => a.media!.group!.index - b.media!.group!.index),
    });
  }
  return albums;
}
