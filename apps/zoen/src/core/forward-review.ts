import type { ForwardReview as CanonicalReview } from './generated/ForwardReview';
import type { ForwardOptions } from './generated/ForwardOptions';
import { ownForwardOptions } from './forward-options.ts';

export type ForwardMessageReview = Readonly<{
  targetId: CanonicalReview['target_space'];
  message: CanonicalReview['message'];
  options: Readonly<ForwardOptions>;
  operationId: CanonicalReview['operation_id'];
  sourceRevision: CanonicalReview['source_revision'];
  acceptedMessageId: CanonicalReview['accepted_message_id'];
}>;

function immutable<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) immutable(child);
    Object.freeze(value);
  }
  return value;
}

/** UI owns an immutable JSON view; the Core alone owns every review/acceptance. */
export function ownForwardReview(value: CanonicalReview): ForwardMessageReview {
  return Object.freeze({
    targetId: value.target_space,
    // Exact decimal strings survive; no structuredClone dependency on Hermes.
    message: immutable(JSON.parse(JSON.stringify(value.message)) as CanonicalReview['message']),
    options: ownForwardOptions(value.options),
    operationId: value.operation_id,
    sourceRevision: value.source_revision,
    acceptedMessageId: value.accepted_message_id,
  });
}
