import * as Clipboard from 'expo-clipboard';
import { messageCard } from '../core/message-presentation';
import { messageText } from '../core/presentation';
import type { TimelineEntry } from '../core/types';

/** Shared text policy for the context menu and multi-message selection. */
export function copyableMessageText(entry: TimelineEntry): string {
  if (entry.deleted || entry.voice) return '';
  const card = messageCard(entry);
  if (card && card.kind_id !== 'file') return '';
  const text = messageText(entry);
  return text.trim() ? text : '';
}

export function copyMessageText(entry: TimelineEntry) {
  const text = copyableMessageText(entry);
  if (text) void Clipboard.setStringAsync(text).catch(() => {});
}
