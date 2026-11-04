import type { DurableAttachmentRuntime } from './durable-attachment-contract';
import { MediaDraftFailure } from './media-draft-contract.ts';

type Context = Pick<DurableAttachmentRuntime, 'channel' | 'store' | 'current'>;
/** Bind publication to a persisted normal editor, then consume its atomic completion receipt. */
export async function prepareMediaReplacement({ channel, store, current }: Context) {
  if (!channel?.scope.edit_message_id) return null;
  const normal = store.messageDrafts.channel(channel.scope.space_id, channel.scope.thread_root_id);
  if (!normal) throw new MediaDraftFailure('closed');
  const { controller, scope } = normal;
  await controller.activate(scope);
  await controller.flush(scope);
  const snapshot = controller.getSnapshot(scope);
  const before = channel.getSnapshot();
  if (!current() || before.content?.kind !== 'attachments') throw new MediaDraftFailure('closed');
  if (!before.frozen) {
    if (
      snapshot.edit?.message_id !== channel.scope.edit_message_id ||
      snapshot.edit.source_revision !== before.content.edit_revision ||
      !controller.isPersisted(scope)
    )
      throw new MediaDraftFailure('conflict');
    await channel.save({ ...before.content, normal_revision: snapshot.persistedRevision });
  }
  const bound = channel.getSnapshot().content;
  const revision = bound?.kind === 'attachments' ? bound.normal_revision : undefined;
  if (!current()) throw new MediaDraftFailure('closed');
  return async () => {
    const latest = controller.getSnapshot(scope);
    if (
      !current() ||
      !store.messageDrafts.isCurrent(normal) ||
      !revision ||
      latest.persistedRevision !== revision ||
      latest.editRevision !== snapshot.editRevision ||
      latest.editContextRevision !== snapshot.editContextRevision ||
      !controller.isPersisted(scope)
    )
      return;
    await controller.acceptMediaEdit(scope, revision);
  };
}
