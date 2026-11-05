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
  const hasEdit = Object.hasOwn(receipt, 'media_edit');
  keys(receipt, [
    'input_revision',
    'messages',
    ...(hasReply ? ['reply'] : []),
    ...(hasEdit ? ['media_edit'] : []),
  ]);
  if (hasEdit) {
    const edit = object(receipt.media_edit);
    keys(edit, ['expected_identity_id', 'expected_device_id', 'space_id', 'message_id', 'edit']);
    if (
      ['expected_identity_id', 'expected_device_id', 'space_id', 'message_id'].some(
        (key) => typeof edit[key] !== 'string' || !edit[key] || !target(edit[key]),
      )
    )
      invalid();
    const change = object(edit.edit);
    keys(change, ['mutation_id', 'expected_revision', 'text', 'replacement']);
    const replacement = object(change.replacement);
    keys(replacement, ['item_id', 'version', 'sha256']);
    if (
      !id(change.mutation_id) ||
      !target(change.expected_revision) ||
      !change.expected_revision ||
      typeof change.text !== 'string' ||
      typeof replacement.item_id !== 'string' ||
      !target(replacement.item_id) ||
      !replacement.item_id ||
      !Number.isSafeInteger(replacement.version) ||
      (replacement.version as number) < 1 ||
      typeof replacement.sha256 !== 'string' ||
      !/^[0-9a-f]{64}$/.test(replacement.sha256)
    )
      invalid();
    if (
      !Array.isArray(receipt.messages) ||
      receipt.messages.length !== 1 ||
      receipt.messages[0] !== edit.message_id ||
      receipt.reply != null
    )
      invalid();
  }
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
