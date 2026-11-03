import type { ReplySelection } from '../core/generated/ReplySelection';
import type { SharedReplyDraft } from '../core/generated/SharedReplyDraft';
import {
  immutableReplySelection,
  immutableSharedReply,
  sameReplyFields,
} from '../core/reply-context.ts';

/** A private projection of Core-reviewed context, never publication authority. */
export type MediaReplyFields = Readonly<{
  reply_quote?: ReplySelection | null;
  shared_reply?: SharedReplyDraft | null;
  reply_intent?: string | null;
}>;
export type MediaReplyOptions = Readonly<{
  replyQuote?: ReplySelection | null;
  sharedReply?: SharedReplyDraft | null;
  replyIntent?: string | null;
}>;
export type MediaReplyTarget = MediaReplyFields & Readonly<{ reply_to: string | null }>;

export function captureMediaReply(options: MediaReplyOptions): MediaReplyFields {
  return Object.freeze({
    ...(options.replyQuote ? { reply_quote: immutableReplySelection(options.replyQuote) } : {}),
    ...(options.sharedReply ? { shared_reply: immutableSharedReply(options.sharedReply) } : {}),
    ...(options.replyIntent ? { reply_intent: options.replyIntent } : {}),
  });
}
export function copyMediaReply(value: MediaReplyFields): MediaReplyFields {
  return captureMediaReply({
    replyQuote: value.reply_quote,
    sharedReply: value.shared_reply,
    replyIntent: value.reply_intent,
  });
}
export function sameMediaReply(a: MediaReplyFields, b: MediaReplyFields): boolean {
  return sameReplyFields(a, b);
}
