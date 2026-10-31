import type { ReplySelection } from './generated/ReplySelection';
import type { SharedReplyDraft } from './generated/SharedReplyDraft';
export type ReplyContextFields = Readonly<{
  reply_to?: string | null;
  reply_quote?: ReplySelection | null;
  shared_reply?: SharedReplyDraft | null;
  reply_intent?: string | null;
}>;
export function sameReplyFields(a: ReplyContextFields, b: ReplyContextFields): boolean {
  return (
    (a.reply_intent ?? null) === (b.reply_intent ?? null) &&
    sameQuote(a.reply_quote, b.reply_quote) &&
    sameShared(a.shared_reply, b.shared_reply)
  );
}
function sameQuote(
  a: ReplySelection | null | undefined,
  b: ReplySelection | null | undefined,
): boolean {
  if (!a || !b) return a == null && b == null;
  return (
    a.source_id === b.source_id &&
    a.revision === b.revision &&
    a.text === b.text &&
    a.spans.length === b.spans.length &&
    a.spans.every(
      (span, i) =>
        span.start_utf16 === b.spans[i]!.start_utf16 && span.end_utf16 === b.spans[i]!.end_utf16,
    )
  );
}
function sameShared(
  a: SharedReplyDraft | null | undefined,
  b: SharedReplyDraft | null | undefined,
): boolean {
  if (!a || !b) return a == null && b == null;
  return (
    a.space_id === b.space_id &&
    a.message_id === b.message_id &&
    a.revision === b.revision &&
    a.author_id === b.author_id &&
    a.author_name === b.author_name &&
    a.text === b.text &&
    sameQuote(a.selection, b.selection)
  );
}
export function sameReplyContext(a: ReplyContextFields, b: ReplyContextFields): boolean {
  return (a.reply_to ?? null) === (b.reply_to ?? null) && sameReplyFields(a, b);
}
export function immutableReplySelection(
  value: ReplySelection | null | undefined,
): ReplySelection | null {
  if (!value) return null;
  const copy = { ...value, spans: value.spans.map((span) => Object.freeze({ ...span })) };
  Object.freeze(copy.spans);
  return Object.freeze(copy);
}
export function immutableSharedReply(
  value: SharedReplyDraft | null | undefined,
): SharedReplyDraft | null {
  if (!value) return null;
  return Object.freeze({
    ...value,
    ...(value.selection
      ? { selection: immutableReplySelection(value.selection) ?? undefined }
      : {}),
  });
}
