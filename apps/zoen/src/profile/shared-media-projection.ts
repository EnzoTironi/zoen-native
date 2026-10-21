import type { ItemDetail, TimelineEntry } from '../core/types.ts';
export type SharedMediaTab = 'media' | 'files' | 'audio' | 'links' | 'apps';
export const sharedMediaTabs: readonly SharedMediaTab[] = [
  'media',
  'files',
  'audio',
  'links',
  'apps',
];
export function sharedMediaTab(value: unknown): SharedMediaTab {
  return typeof value === 'string' && sharedMediaTabs.includes(value as SharedMediaTab)
    ? (value as SharedMediaTab)
    : 'media';
}
/** Project only the current, authorized core Items. Home visibility never grants/revokes access. */
export function sharedItems(
  items: readonly ItemDetail[],
  spaceId: string,
  tab: SharedMediaTab,
): ItemDetail[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (item.space_id !== spaceId || seen.has(item.id)) return false;
    seen.add(item.id);
    if (tab === 'apps') return Boolean(item.app);
    if (!item.file) return false;
    const mime = item.file.mime.toLowerCase();
    if (tab === 'media') return mime.startsWith('image/') || mime.startsWith('video/');
    if (tab === 'audio') return mime.startsWith('audio/');
    return (
      tab === 'files' &&
      !mime.startsWith('image/') &&
      !mime.startsWith('video/') &&
      !mime.startsWith('audio/')
    );
  });
}
export type SharedLink = Readonly<{
  id: string;
  messageId: string;
  url: string;
  host: string;
  text: string;
}>;
/** Loaded canonical history only, with explicit older-history navigation. Never fetch link previews. */
export function sharedLinks(entries: readonly TimelineEntry[]): SharedLink[] {
  const output: SharedLink[] = [];
  for (const entry of [...entries].reverse()) {
    if (entry.deleted || !('Message' in entry.kind)) continue;
    const text = entry.kind.Message.text;
    const seen = new Set<string>();
    for (const match of text.matchAll(/https?:\/\/[^\s<>]+/gi)) {
      const raw = match[0].replace(/[.,!?;:)\]}]+$/, '');
      try {
        const url = new URL(raw);
        if (
          !['https:', 'http:'].includes(url.protocol) ||
          url.username ||
          url.password ||
          seen.has(url.href)
        )
          continue;
        seen.add(url.href);
        output.push({
          id: `${entry.id}:${url.href}`,
          messageId: entry.id,
          url: url.href,
          host: url.hostname,
          text,
        });
      } catch {
        /* Malformed message text is not an external navigation capability. */
      }
    }
  }
  return output;
}
