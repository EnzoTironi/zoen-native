import { randomUUID } from 'expo-crypto';
import { pickAttachmentAssets, type AttachmentPickKind } from './attachment-picker';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import type {
  DurableAttachmentActions,
  DurableAttachmentRuntime,
} from './durable-attachment-contract';
import { AttachmentDraftFailure, type AttachmentAsset } from './attachment-draft';
import type { MediaDraftAttachments } from './media-draft-types';

type Context = Pick<
  DurableAttachmentRuntime,
  | 'channel'
  | 'current'
  | 'runtime'
  | 'state'
  | 'review'
  | 'publish'
  | 'photos'
  | 'store'
  | 'admission'
> &
  Pick<DurableAttachmentActions, 'failure' | 'openApproval' | 'edit' | 'send'>;
/** One selection replaces the edit journal atomically; the normal composer owns another scope. */
export async function pickMediaReplacement(context: Context, kind: AttachmentPickKind) {
  const { channel, current, runtime, photos, admission, publish, failure, review } = context;
  const target = admission.current.mediaEdit;
  if (
    !target ||
    !channel ||
    !current() ||
    runtime.picking ||
    runtime.submitting ||
    channel.scope.edit_message_id !== target.id
  )
    return;
  const before = channel.getSnapshot();
  if (before.phase !== 'ready' || before.frozen || before.saving || before.sending) return;
  if (review.getSnapshot().mutationId) {
    try {
      await review.close();
    } catch (error) {
      failure(error);
      return;
    }
  }
  if (!current()) return;
  runtime.picking = true;
  publish({ picking: true, error: null });
  let selected: AttachmentAsset[] = [];
  let adopted = false;
  let next: MediaDraftAttachments | null = null;
  let directSend = false;
  const presentation = runtime.approval;
  try {
    await sensitiveCacheWork.run(async () => {
      const caption =
        before.content?.kind === 'attachments'
          ? (before.content.items[0]?.caption ?? target.text)
          : target.text;
      selected =
        kind === 'photo'
          ? await photos.pick(1, current, { sendAsFileAllowed: false, caption })
          : await pickAttachmentAssets(kind, 1, current);
      if (
        !selected.length ||
        !current() ||
        admission.current.mediaEdit?.id !== target.id ||
        channel.getSnapshot().value?.revision !== before.value?.revision
      )
        return;
      if (selected.length !== 1) throw new AttachmentDraftFailure('limit');
      const asset = selected[0],
        id = randomUUID();
      const submission = kind === 'photo' ? photos.submission() : null;
      next = {
        kind: 'attachments',
        reply_to: null,
        edit_revision: target.revision,
        items: [
          {
            asset_id: id,
            mutation_id: id,
            caption: submission?.captions?.[0] ?? submission?.caption ?? caption,
          },
        ],
      };
      await channel.save(next, [{ id, uri: asset.uri, name: asset.name, mime: asset.mime }]);
      if (!current()) return;
      const old = [...runtime.previews.values()];
      runtime.previews.clear();
      runtime.previews.set(id, {
        ...asset,
        release: async () => {
          await context.store.driver.media?.releaseLocalFile(asset.uri);
          discardCacheCopy(asset.uri);
        },
      });
      adopted = true;
      publish({ selected: id, previews: new Map(runtime.previews) });
      await Promise.allSettled(old.map((preview) => preview.release()));
      directSend = Boolean(submission?.send);
    });
  } catch (error) {
    failure(error);
  } finally {
    if (!adopted)
      for (const asset of selected) {
        try {
          await context.store.driver.media?.releaseLocalFile(asset.uri);
          discardCacheCopy(asset.uri);
        } catch (error) {
          failure(error);
        }
      }
    runtime.picking = false;
    publish({ picking: false });
  }
  if (adopted && next && current() && runtime.approval === presentation) {
    if (directSend) await context.send(next, presentation);
    else if (/^(image|video)\//.test(selected[0].mime)) context.openApproval();
  }
}
