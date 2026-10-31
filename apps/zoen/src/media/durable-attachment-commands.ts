import { Keyboard } from 'react-native';
import { t } from '../i18n';
import { supportsMediaApproval } from './attachment-approval';
import { AttachmentDraftFailure } from './attachment-draft';
import { sensitiveCacheWork } from './cache-work';
import type {
  DurableAttachmentActions,
  DurableAttachmentRuntime,
} from './durable-attachment-contract';
import type { ApprovalPresentation } from './durable-attachment-types';
import type { MediaDraftAttachments } from './media-draft-types';
import { mediaPlayback } from './playback-owner';
import { sameMediaReply } from './media-reply';
import { mediaAcceptedIntent } from './media-acceptance';

const sameAttachmentSelection = (value: MediaDraftAttachments, observed: MediaDraftAttachments) =>
  value.reply_to === observed.reply_to &&
  sameMediaReply(value, observed) &&
  (value.options?.presentation ?? 'media') === (observed.options?.presentation ?? 'media') &&
  (value.options?.grouping ?? 'separate') === (observed.options?.grouping ?? 'separate') &&
  value.items.length === observed.items.length &&
  value.items.every(
    (item, index) =>
      item.asset_id === observed.items[index]?.asset_id &&
      item.mutation_id === observed.items[index]?.mutation_id,
  );
export function failure(context: Pick<DurableAttachmentRuntime, 'publish'>, reason: unknown) {
  const { publish } = context;
  publish({
    error:
      reason instanceof AttachmentDraftFailure
        ? t(
            reason.kind === 'limit'
              ? 'media.attachmentDraftLimit'
              : reason.kind === 'size'
                ? 'media.attachmentTooLarge'
                : 'media.attachmentDraftUnavailable',
          )
        : reason instanceof Error
          ? reason.message
          : t('media.attachmentFailed'),
  });
}

export function openApproval(
  context: Pick<DurableAttachmentRuntime, 'channel' | 'current' | 'runtime' | 'publish'>,
  id?: string,
) {
  const { channel, current, runtime, publish } = context;
  if (
    !supportsMediaApproval ||
    !channel ||
    !current() ||
    !runtime.foreground ||
    runtime.picking ||
    runtime.approval?.closing
  )
    return;
  const latest = channel.getSnapshot();
  if (latest.content?.kind !== 'attachments' || !latest.content.items.length || latest.sending)
    return;
  Keyboard.dismiss();
  if (!runtime.approval) {
    mediaPlayback.pauseAll();
    runtime.approval = { closing: false };
  }
  const item = latest.content.items.find((item) => item.mutation_id === id);
  publish({ approval: runtime.approval, ...(item ? { selected: item.asset_id } : {}) });
}

export async function closeApproval(
  context: Pick<DurableAttachmentRuntime, 'runtime' | 'publish' | 'review' | 'retireApproval'> &
    Pick<DurableAttachmentActions, 'failure'>,
) {
  const { runtime, publish, review, retireApproval, failure } = context;
  const presentation = runtime.approval;
  if (!presentation || presentation.closing) return;
  presentation.closing = true;
  mediaPlayback.pauseAll();
  publish({ approval: presentation });
  Keyboard.dismiss();
  try {
    await review.close();
    if (runtime.approval === presentation) retireApproval();
  } catch (reason) {
    if (runtime.approval === presentation) {
      presentation.closing = false;
      publish({ approval: presentation });
      failure(reason);
    }
  }
}

export async function cancel(
  context: Pick<
    DurableAttachmentRuntime,
    'channel' | 'current' | 'state' | 'runtime' | 'own' | 'review' | 'publish' | 'retireApproval'
  > &
    Pick<DurableAttachmentActions, 'failure'>,
) {
  const { channel, current, state, runtime, own, review, failure, publish, retireApproval } =
    context;
  if (!channel || !current() || state?.sending || runtime.picking || own?.clearing) return;
  if (review.getSnapshot().mutationId) {
    try {
      await review.close();
    } catch (reason) {
      failure(reason);
      return;
    }
  }
  if (!current()) return;
  publish({ clearing: true, error: null });
  try {
    await channel.clear();
    retireApproval();
  } catch (reason) {
    failure(reason);
  } finally {
    publish({ clearing: false });
  }
}

/** Explicit composer actions finish the picker phase, keeping the same assets and mutation IDs. */
function completeGallery(value: MediaDraftAttachments): MediaDraftAttachments {
  if (!value.gallery) return value;
  const next = { ...value };
  delete next.gallery;
  return next;
}

export async function remove(
  context: Pick<DurableAttachmentRuntime, 'channel' | 'current' | 'review' | 'retireApproval'> &
    Pick<DurableAttachmentActions, 'failure'>,
  id: string,
) {
  const { channel, current, review, failure, retireApproval } = context;
  if (!channel || !current()) return;
  if (review.getSnapshot().mutationId) {
    try {
      await review.close();
    } catch (reason) {
      failure(reason);
      return;
    }
  }
  if (!current()) return;
  const latest = channel.getSnapshot();
  const currentContent = latest.content;
  if (latest.frozen || currentContent?.kind !== 'attachments') return;
  const items = currentContent.items.filter((item) => item.asset_id !== id);
  try {
    await channel.save(
      items.length
        ? {
            ...completeGallery(currentContent),
            items,
            ...(items.length === 1 && currentContent.options?.grouping === 'grouped'
              ? {
                  options: {
                    presentation: currentContent.options.presentation,
                    grouping: 'separate',
                  },
                }
              : {}),
          }
        : null,
    );
    if (!items.length) retireApproval();
  } catch (reason) {
    failure(reason);
  }
}

export async function reorder(
  context: Pick<DurableAttachmentRuntime, 'channel' | 'current' | 'runtime' | 'review'> &
    Pick<DurableAttachmentActions, 'failure'>,
  ids: readonly string[],
) {
  const { channel, current, runtime, review, failure } = context;
  if (
    !channel ||
    !current() ||
    runtime.picking ||
    runtime.submitting ||
    runtime.approval?.closing ||
    review.getSnapshot().mutationId
  )
    return;
  const latest = channel.getSnapshot();
  if (latest.frozen || latest.content?.kind !== 'attachments') return;
  const items = latest.content.items;
  if (ids.length !== items.length || new Set(ids).size !== ids.length) return;
  const byId = new Map(items.map((item) => [item.mutation_id, item]));
  if (ids.some((id) => !byId.has(id))) return;
  try {
    await channel.save({
      ...completeGallery(latest.content),
      items: ids.map((id) => byId.get(id)!),
    });
  } catch (reason) {
    failure(reason);
  }
}

export async function edit(
  context: Pick<
    DurableAttachmentRuntime,
    'channel' | 'current' | 'runtime' | 'review' | 'publish'
  > &
    Pick<DurableAttachmentActions, 'failure'>,
  mutationId: string,
  presentation: ApprovalPresentation,
  finalCaption?: string,
) {
  const { channel, current, runtime, review, publish, failure } = context;
  if (
    !channel ||
    !current() ||
    runtime.picking ||
    runtime.submitting ||
    review.getSnapshot().mutationId ||
    runtime.approval !== presentation ||
    presentation.closing
  )
    return;
  const latest = channel.getSnapshot();
  const before = latest.content;
  if (before?.kind !== 'attachments' || latest.frozen || latest.sending || latest.reviewing) return;
  const index = before.items.findIndex((item) => item.mutation_id === mutationId);
  if (index < 0) return;
  if (
    finalCaption !== undefined &&
    (typeof finalCaption !== 'string' || finalCaption.length > 4096 || finalCaption.includes('\0'))
  )
    return;
  runtime.submitting = true;
  publish({ error: null });
  try {
    if (
      before.gallery ||
      (finalCaption !== undefined && before.items[index]!.caption !== finalCaption)
    )
      await channel.save({
        ...completeGallery(before),
        items: before.items.map((item) =>
          item.mutation_id === mutationId && finalCaption !== undefined
            ? { ...item, caption: finalCaption }
            : item,
        ),
      });
    const saved = channel.getSnapshot();
    if (
      !current() ||
      runtime.approval !== presentation ||
      presentation.closing ||
      saved.content?.kind !== 'attachments' ||
      !sameAttachmentSelection(saved.content, before)
    )
      return;
    await review.open(mutationId);
    const after = channel.getSnapshot().content;
    if (
      current() &&
      runtime.approval === presentation &&
      !presentation.closing &&
      after?.kind === 'attachments' &&
      after.reply_to === before.reply_to &&
      sameMediaReply(after, before) &&
      after.items.length === before.items.length &&
      after.items.every(
        (item, position) =>
          position === index || item.mutation_id === before.items[position]?.mutation_id,
      )
    )
      publish({ selected: after.items[index]!.asset_id });
  } catch (reason) {
    failure(reason);
  } finally {
    runtime.submitting = false;
    publish({});
  }
}

export async function send(
  context: Pick<
    DurableAttachmentRuntime,
    | 'channel'
    | 'current'
    | 'runtime'
    | 'review'
    | 'admission'
    | 'publish'
    | 'retireApproval'
    | 'store'
  > &
    Pick<DurableAttachmentActions, 'failure'>,
  observed: MediaDraftAttachments,
  presentation: ApprovalPresentation | null,
  finalCaption?: { mutationId: string; text: string },
) {
  const { channel, current, runtime, review, admission, publish, retireApproval, store, failure } =
    context;
  // A stale Send action must not dismiss review and queue delivery behind it.
  if (
    !channel ||
    !current() ||
    runtime.picking ||
    runtime.submitting ||
    review.getSnapshot().mutationId ||
    runtime.approval !== presentation ||
    presentation?.closing
  )
    return;
  const latest = channel.getSnapshot();
  if (latest.content?.kind !== 'attachments' || latest.sending || latest.reviewing) return;
  // Caption saves can drain, but this action belongs only to its rendered selection.
  if (!sameAttachmentSelection(latest.content, observed)) return;
  if (
    finalCaption &&
    (typeof finalCaption.text !== 'string' ||
      finalCaption.text.length > 4096 ||
      finalCaption.text.includes('\0'))
  )
    return;
  const sent = admission.current.onSent;
  runtime.submitting = true;
  publish({ error: null });
  try {
    if (finalCaption) {
      const item = latest.content.items.find(
        (value) => value.mutation_id === finalCaption.mutationId,
      );
      if (!item) return;
      if (item.caption !== finalCaption.text) {
        if (latest.frozen) return;
        // The final native UITextView value can be newer than React's last
        // caption callback. Journal it through the same CAS writer before Send.
        await channel.save({
          ...completeGallery(latest.content),
          items: latest.content.items.map((value) =>
            value.mutation_id === finalCaption.mutationId
              ? { ...value, caption: finalCaption.text }
              : value,
          ),
        });
      }
      const after = channel.getSnapshot();
      if (
        !current() ||
        runtime.approval !== presentation ||
        presentation?.closing ||
        review.getSnapshot().mutationId ||
        after.content?.kind !== 'attachments' ||
        !sameAttachmentSelection(after.content, observed)
      )
        return;
    }
    const pending = channel.getSnapshot();
    if (pending.content?.kind === 'attachments' && pending.content.gallery && !pending.frozen) {
      await channel.save(completeGallery(pending.content));
      const after = channel.getSnapshot();
      if (
        !current() ||
        runtime.approval !== presentation ||
        presentation?.closing ||
        after.content?.kind !== 'attachments' ||
        !sameAttachmentSelection(after.content, observed)
      )
        return;
    }
    const result = await sensitiveCacheWork.run(() => channel.send());
    const accepted = mediaAcceptedIntent(result.draft);
    if (result.entries.length || accepted?.messages.length) {
      retireApproval();
      if (!runtime.closed && store.mediaDrafts.isCurrent(channel)) sent?.(accepted?.reply);
      void store.refresh().catch(() => {});
    }
  } catch (reason) {
    failure(reason);
  } finally {
    runtime.submitting = false;
    publish({});
  }
}
