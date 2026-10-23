import { randomUUID } from 'expo-crypto';
import { AttachmentDraftFailure } from './attachment-draft';
import { AttachmentReview } from './attachment-review';
import type { DurableAttachmentRuntime } from './durable-attachment-contract';

export function createDurableAttachmentReview(
  context: Pick<DurableAttachmentRuntime, 'channel' | 'current' | 'store' | 'runtime' | 'setLocal'>,
) {
  const { channel, current, store, runtime, setLocal } = context;
  return new AttachmentReview({
    token: randomUUID,
    prepare: async (mutationId) => {
      if (!channel || !current()) return null;
      await channel.flush();
      if (!current()) return null;
      const latest = channel.getSnapshot();
      if (latest.phase !== 'ready' || latest.sending || latest.content?.kind !== 'attachments')
        return null;
      const item = latest.content.items.find((value) => value.mutation_id === mutationId);
      const asset = latest.value?.assets.find((value) => value.id === item?.asset_id);
      if (!item || !asset || !latest.value) return null;
      return Object.freeze({
        scope: channel.scope,
        revision: latest.value.revision,
        assetId: item.asset_id,
        mutationId,
        sha256: asset.sha256,
        replyTo: latest.content.reply_to,
      });
    },
    current: (request) => {
      if (!channel || !current() || request.scope !== channel.scope) return false;
      const latest = channel.getSnapshot();
      const content = latest.content;
      return Boolean(
        latest.phase === 'ready' &&
        !latest.sending &&
        content?.kind === 'attachments' &&
        content.reply_to === request.replyTo &&
        content.items.some(
          (item) => item.mutation_id === request.mutationId && item.asset_id === request.assetId,
        ) &&
        latest.value?.assets.some(
          (asset) => asset.id === request.assetId && asset.sha256 === request.sha256,
        ),
      );
    },
    preview: (request, token) => {
      const port = store.driver.media?.drafts;
      if (!port?.preview || !port.cancelPreview) throw new AttachmentDraftFailure('unavailable');
      const latest = channel?.getSnapshot();
      const asset = latest?.value?.assets.find((value) => value.id === request.assetId);
      const editable =
        asset && /^(image\/(jpeg|png|heic|heif)|video\/(mp4|quicktime|x-m4v))$/.test(asset.mime);
      if (port.edit && channel && editable && !latest?.frozen) {
        const content = latest?.content;
        const position =
          content?.kind === 'attachments'
            ? content.items.findIndex((item) => item.asset_id === request.assetId)
            : -1;
        return channel.edit(request.assetId, token).then(() => {
          const updated = channel.getSnapshot().content;
          const selected =
            updated?.kind === 'attachments' && position >= 0
              ? updated.items[position]?.asset_id
              : null;
          if (selected && !runtime.closed)
            setLocal((previous) =>
              previous.channel === channel ? { ...previous, selected } : previous,
            );
        });
      }
      return port.preview(request.scope, request.revision, request.assetId, token);
    },
    cancel: (token) => store.driver.media?.drafts?.cancelPreview?.(token) ?? Promise.resolve(),
  });
}
