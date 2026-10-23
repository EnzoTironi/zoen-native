import type { TimelineEntry } from './types';
import emojiRegex from 'emoji-regex';

export function isSystemMessage(entry: TimelineEntry) {
  return 'System' in entry.kind || 'Background' in entry.kind;
}

export function sameMessageDay(previous: TimelineEntry | undefined, entry: TimelineEntry): boolean {
  if (!previous) return false;
  const before = new Date(Number(previous.at_ms));
  const after = new Date(Number(entry.at_ms));
  return (
    Number.isFinite(before.getTime()) &&
    Number.isFinite(after.getTime()) &&
    before.getFullYear() === after.getFullYear() &&
    before.getMonth() === after.getMonth() &&
    before.getDate() === after.getDate()
  );
}

/** Telegram's strict ten-minute boundary projected onto the fields Zoen actually owns.
 * Forward attribution is not exposed by this DTO, so never infer equal forward authors.
 * Zoen keeps its real thread and civil-day boundaries rather than copying upstream's
 * hard-coded sameThread=true or merging through a date separator. */
export function messagesAreGrouped(previous: TimelineEntry | undefined, entry: TimelineEntry) {
  if (!previous || !('Message' in previous.kind) || !('Message' in entry.kind)) return false;
  const elapsed = Number(entry.at_ms) - Number(previous.at_ms);
  return (
    !previous.deleted &&
    !entry.deleted &&
    !previous.forwarded &&
    !entry.forwarded &&
    previous.author.id === entry.author.id &&
    previous.author.is_me === entry.author.is_me &&
    previous.in_thread === entry.in_thread &&
    previous.kind.Message.card?.kind_id !== 'app' &&
    entry.kind.Message.card?.kind_id !== 'app' &&
    elapsed >= 0 &&
    elapsed < 600_000 &&
    sameMessageDay(previous, entry)
  );
}

/** Static emoji use the same grapheme boundaries as the text renderer, including
 * ZWJ families, flags, skin tones and keycaps. Ordinary digits and punctuation
 * never become oversized emoji just because they have an emoji variation. */
export function largeEmojiFontSize(text: string, enabled: boolean): number | null {
  if (!enabled || !text || text.length > 1024) return null;
  const compact = text.replace(/[ \n]/g, '');
  const symbols = compact.match(emojiRegex()) ?? [];
  if (symbols.length < 1 || symbols.length > 9 || symbols.join('') !== compact) return null;
  const lines = text.split('\n');
  const length = Math.max(
    lines.length,
    ...lines.map((line) => (line.match(emojiRegex()) ?? []).length),
  );
  // Telegram ChatMessageAnimatedStickerItemNode.fontSizeForEmojiString.
  const multipliers = [1, 0.84, 0.69, 0.53, 0.46, 0.38, 0.32, 0.27, 0.24];
  return Math.floor(94 * (multipliers[length - 1] ?? 0.21));
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
    (!entry.kind.Message.card || entry.kind.Message.card.kind_id === 'file')
  );
}

export function messageCanInteract(entry: TimelineEntry) {
  return !entry.deleted && 'Message' in entry.kind;
}

/** Selection only; the core rechecks the signed Failed outbox and both drafts. */
export function messageCanRecoverMedia(entry: TimelineEntry) {
  return (
    messageCanInteract(entry) &&
    entry.author.is_me &&
    entry.delivery === 'Failed' &&
    Boolean(entry.voice || messageCard(entry)?.kind_id === 'file')
  );
}

export function messageHasAttachment(entry: TimelineEntry) {
  return !entry.deleted && Boolean(entry.voice || messageCard(entry));
}

/** Human App shares present the canonical cover alone; Agent text remains authored content. */
export function messageHasStandaloneAppCard(entry: TimelineEntry): boolean {
  const card = messageCard(entry);
  return entry.author.kind === 'Person' && card?.kind_id === 'app' && card.app != null;
}
