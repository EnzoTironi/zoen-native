import type { NormalDraftChannel } from './normal-message-draft-service';
import type { ReplySelection } from './generated/ReplySelection';

/** A temporary interaction fence over the canonical draft, never another draft store. */
export function captureReplyOptions(
  channel: NormalDraftChannel,
  admitted: () => boolean,
  invalidated?: () => void,
) {
  const initial = channel.controller.getSnapshot(channel.scope);
  let retired = false;
  const current = () => {
    if (retired) return false;
    const value = channel.controller.getSnapshot(channel.scope);
    if (
      !admitted() ||
      channel.controller.hasPendingSend(channel.scope) ||
      !value.loaded ||
      value.sending ||
      value.replyRevision !== initial.replyRevision
    ) {
      retired = true;
      invalidated?.();
    }
    return !retired;
  };
  const detach = channel.controller.subscribe(current);
  return {
    initial,
    current,
    retire: () => {
      retired = true;
      detach();
    },
  };
}

/** Only an exact durable source revision can carry a selection into the destination.
 * The route contains identifiers; the excerpt remains inside the encrypted draft. */
export function sharedReplySelection(
  channel: NormalDraftChannel,
  draftRevision: string,
  messageId: string,
): ReplySelection | null {
  const value = channel.controller.getSnapshot(channel.scope);
  if (
    !channel.controller.isPersisted(channel.scope) ||
    value.sending ||
    value.saving ||
    value.persistedRevision !== draftRevision ||
    value.sharedReply ||
    value.replyTo !== messageId
  )
    throw new Error('Reply source changed');
  return value.replyQuote;
}
