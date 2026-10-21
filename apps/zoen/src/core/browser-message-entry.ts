import type { Message } from '../../../../packages/zoen-ui/web/messages';
import type { TimelineEntry } from './types';
import { browserMediaEntry } from './browser-media-entry.ts';

/** A typed view of this browser's signed history, never a text heuristic. */
export function browserMessageEntry(
  message: Message,
): Pick<TimelineEntry, 'kind' | 'voice' | 'media'> {
  if (message.item_edit) {
    const { item_id, title, version, note, is_undo } = message.item_edit;
    return { kind: { ItemEdited: { item_id, title, version, note, is_undo } }, voice: null };
  }
  if (message.card && !message.deleted)
    return { kind: { Message: { text: message.text, card: message.card } }, voice: null };
  return {
    ...browserMediaEntry(message),
    ...(message.media && !message.deleted ? { media: message.media } : {}),
  };
}
