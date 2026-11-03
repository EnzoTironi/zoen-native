import { id, invalid, keys, object, target } from './media-draft-values.ts';
import { mediaReplyValue } from './media-reply-contract.ts';
import type { MediaReplyTarget } from './media-reply';

export type AcceptedMediaReply = MediaReplyTarget;
export type AcceptedMediaIntent = Readonly<{
  input_revision: string;
  messages: readonly string[];
  reply?: AcceptedMediaReply;
}>;
/** Core's sealed receipt proves acceptance; a visible entry is only a projection. */
export function mediaAcceptedIntent(
  value: { accepted_intent_json?: string | null } | null | undefined,
): AcceptedMediaIntent | null {
  if (value?.accepted_intent_json == null) return null;
  const receipt = object(JSON.parse(value.accepted_intent_json));
  const hasReply = Object.hasOwn(receipt, 'reply');
  keys(receipt, ['input_revision', 'messages', ...(hasReply ? ['reply'] : [])]);
  if (
    !id(receipt.input_revision) ||
    receipt.input_revision === '0' ||
    !Array.isArray(receipt.messages) ||
    !receipt.messages.length ||
    receipt.messages.length > 10 ||
    receipt.messages.some((message) => !target(message) || !message) ||
    new Set(receipt.messages).size !== receipt.messages.length
  )
    invalid();
  let reply: AcceptedMediaReply | undefined;
  if (hasReply && receipt.reply != null) {
    const raw = object(receipt.reply);
    keys(raw, [
      'reply_to',
      ...['reply_quote', 'shared_reply', 'reply_intent'].filter((key) => Object.hasOwn(raw, key)),
    ]);
    if (!target(raw.reply_to)) invalid();
    reply = Object.freeze({ reply_to: raw.reply_to, ...mediaReplyValue(raw) });
  }
  return Object.freeze({
    input_revision: receipt.input_revision as string,
    messages: Object.freeze([...(receipt.messages as string[])]),
    ...(reply ? { reply } : {}),
  });
}
