import type { ReplySelection } from '../core/generated/ReplySelection';
import {
  MAX_MEDIA_DRAFT_CONTENT_BYTES,
  boundedText,
  id,
  invalid,
  keys,
  object,
  target,
} from './media-draft-values.ts';
import { captureMediaReply, type MediaReplyFields } from './media-reply.ts';

/** Shape-only admission. The Core authenticates source, version and UTF-16 selection. */
function selectionValue(input: unknown): ReplySelection {
  const value = object(input);
  keys(value, ['source_id', 'revision', 'spans', 'text']);
  if (
    !target(value.source_id) ||
    !value.source_id ||
    !target(value.revision) ||
    !value.revision ||
    !boundedText(value.text, MAX_MEDIA_DRAFT_CONTENT_BYTES) ||
    !Array.isArray(value.spans) ||
    !value.spans.length
  )
    return invalid();
  const spans = value.spans.map((input) => {
    const span = object(input);
    keys(span, ['start_utf16', 'end_utf16']);
    if (
      !Number.isSafeInteger(span.start_utf16) ||
      !Number.isSafeInteger(span.end_utf16) ||
      (span.start_utf16 as number) < 0 ||
      (span.end_utf16 as number) <= (span.start_utf16 as number)
    )
      return invalid();
    return { start_utf16: span.start_utf16 as number, end_utf16: span.end_utf16 as number };
  });
  return { source_id: value.source_id, revision: value.revision, spans, text: value.text };
}

export function mediaReplyValue(raw: Record<string, unknown>): MediaReplyFields {
  if (
    raw.reply_intent != null &&
    (!id(raw.reply_intent) || raw.reply_intent === '0' || (!raw.reply_to && !raw.shared_reply))
  )
    invalid();
  const quote = raw.reply_quote == null ? null : selectionValue(raw.reply_quote);
  let shared = null;
  if (raw.shared_reply != null) {
    const value = object(raw.shared_reply);
    const hasSelection = Object.hasOwn(value, 'selection');
    keys(value, [
      'space_id',
      'message_id',
      'revision',
      'author_id',
      'author_name',
      'text',
      ...(hasSelection ? ['selection'] : []),
    ]);
    for (const name of ['space_id', 'message_id', 'revision', 'author_id'])
      if (!target(value[name]) || !value[name]) invalid();
    if (
      !boundedText(value.author_name, MAX_MEDIA_DRAFT_CONTENT_BYTES) ||
      !boundedText(value.text, MAX_MEDIA_DRAFT_CONTENT_BYTES)
    )
      invalid();
    const selection = value.selection == null ? null : selectionValue(value.selection);
    if (
      selection &&
      (selection.source_id !== value.message_id ||
        selection.revision !== value.revision ||
        selection.text !== value.text)
    )
      invalid();
    shared = {
      space_id: value.space_id as string,
      message_id: value.message_id as string,
      revision: value.revision as string,
      author_id: value.author_id as string,
      author_name: value.author_name as string,
      text: value.text as string,
      ...(selection ? { selection } : {}),
    };
  }
  if ((shared && (quote || raw.reply_to !== null)) || (quote && quote.source_id !== raw.reply_to))
    invalid();
  return captureMediaReply({
    replyQuote: quote,
    sharedReply: shared,
    replyIntent: raw.reply_intent as string | null | undefined,
  });
}
