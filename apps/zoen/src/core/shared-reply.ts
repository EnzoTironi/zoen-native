import type { SharedReplyDraft } from './generated/SharedReplyDraft';
import type { NormalDraftChannel } from './normal-message-draft-service';

/** Choosing a destination only prepares its canonical draft. There is deliberately
 * no publication port here. Every await rechecks the captured account and access. */
export async function prepareSharedReply({
  channel,
  current,
  review,
}: {
  channel: NormalDraftChannel;
  current: () => boolean;
  review: () => Promise<SharedReplyDraft>;
}): Promise<string> {
  const requireCurrent = () => {
    if (!current()) throw new Error('Reply destination is no longer available.');
  };
  requireCurrent();
  await channel.controller.activate(channel.scope);
  requireCurrent();
  const quote = await review();
  requireCurrent();
  if (!channel.controller.setSharedReply(channel.scope, quote))
    throw new Error('The draft is busy. Try again when its send finishes.');
  await channel.controller.flush(channel.scope);
  requireCurrent();
  const draft = channel.controller.getSnapshot(channel.scope);
  if (JSON.stringify(draft.sharedReply) !== JSON.stringify(quote))
    throw new Error('The reply changed while choosing its destination.');
  return draft.persistedRevision;
}
