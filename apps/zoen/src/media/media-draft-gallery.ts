import { boundedText, hasControl, invalid, keys, object } from './media-draft-contract.ts';
import type { MediaDraftAttachment, MediaDraftGallery } from './media-draft-types.ts';

/** Same strict private content contract as media_drafts::gallery in Rust. */
export function galleryValue(
  input: unknown,
  items: readonly MediaDraftAttachment[],
): MediaDraftGallery {
  const raw = object(input);
  keys(raw, ['entries', 'forced_caption']);
  if (
    !Array.isArray(raw.entries) ||
    !raw.entries.length ||
    raw.entries.length > items.length ||
    typeof raw.forced_caption !== 'boolean'
  )
    return invalid();
  const suffix = items.slice(items.length - raw.entries.length);
  if (raw.forced_caption && suffix.slice(1).some((item) => item.caption.length > 0)) invalid();
  const photos = new Set<string>();
  const entries = raw.entries.map((value: unknown, index: number) => {
    const entry = object(value);
    keys(entry, ['asset_id', 'photo_id']);
    if (
      entry.asset_id !== suffix[index]?.asset_id ||
      !boundedText(entry.photo_id, 2048) ||
      !entry.photo_id.startsWith('ph://') ||
      entry.photo_id.length <= 5 ||
      hasControl(entry.photo_id) ||
      photos.has(entry.photo_id)
    )
      return invalid();
    photos.add(entry.photo_id);
    return { asset_id: suffix[index]!.asset_id, photo_id: entry.photo_id };
  });
  return { entries, forced_caption: raw.forced_caption };
}
