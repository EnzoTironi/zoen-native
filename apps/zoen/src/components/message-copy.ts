import * as Clipboard from 'expo-clipboard';
import { copyableMessageText } from '../core/message-copy-text';
import type { TimelineEntry } from '../core/types';
export { copyableMessageText } from '../core/message-copy-text';

export function copyMessageText(entry: TimelineEntry) {
  const text = copyableMessageText(entry);
  if (text) void Clipboard.setStringAsync(text).catch(() => {});
}
