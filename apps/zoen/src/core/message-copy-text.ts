import { messageCard } from './message-presentation.ts';
import { messageText } from './presentation.ts';
import type { TimelineEntry } from './types';

/** Shared text policy for the context menu and multi-message selection. */
export function copyableMessageText(entry: TimelineEntry): string {
  if (entry.deleted || entry.voice) return '';
  const card = messageCard(entry);
  if (card && card.kind_id !== 'file') return '';
  const text = messageText(entry);
  return text.trim() ? text : '';
}
