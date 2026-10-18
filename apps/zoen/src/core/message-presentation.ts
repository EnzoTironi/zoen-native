import type { TimelineEntry } from './types';

export function isSystemMessage(entry: TimelineEntry) {
  return 'System' in entry.kind || 'Background' in entry.kind;
}

/** Group by continuity, not just by author: old messages never inherit a new bubble's tail. */
export function messagesAreGrouped(previous: TimelineEntry | undefined, entry: TimelineEntry) {
  if (!previous || isSystemMessage(previous) || isSystemMessage(entry)) return false;
  const elapsed = Number(entry.at_ms) - Number(previous.at_ms);
  return previous.author.id === entry.author.id && elapsed >= 0 && elapsed < 180_000;
}

export function messageCard(entry: TimelineEntry) {
  return !entry.deleted && 'Message' in entry.kind ? entry.kind.Message.card : null;
}

export function messageCanEdit(entry: TimelineEntry) {
  return (
    entry.author.is_me &&
    !entry.deleted &&
    !entry.voice &&
    'Message' in entry.kind &&
    !entry.kind.Message.card
  );
}

export function messageCanInteract(entry: TimelineEntry) {
  return !entry.deleted && 'Message' in entry.kind;
}

export function messageHasAttachment(entry: TimelineEntry) {
  return !entry.deleted && Boolean(entry.voice || messageCard(entry));
}
