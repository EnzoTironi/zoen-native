import { randomUUID } from 'expo-crypto';
import {
  AttachmentDraftFailure,
  MAX_ATTACHMENT_ITEMS,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
} from './attachment-draft';
import { pickAttachmentAssets, type AttachmentPickKind } from './attachment-picker';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import type {
  DurableAttachmentActions,
  DurableAttachmentRuntime,
} from './durable-attachment-contract';
import type { MediaDraftAttachments, MediaDraftIngest } from './media-draft-types';
import { mediaPlayback } from './playback-owner';

export async function pick(
  context: Pick<
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
    Pick<DurableAttachmentActions, 'failure' | 'openApproval' | 'edit' | 'send'>,
  kind: AttachmentPickKind,
) {
  const {
    channel,
    current,
    runtime,
    state,
    review,
    failure,
    publish,
    photos,
    store,
    admission,
    openApproval,
    edit,
    send,
  } = context;
  if (
    !channel ||
    !current() ||
    runtime.picking ||
    state?.phase !== 'ready' ||
    state.frozen ||
    (state.content && state.content.kind !== 'attachments')
  )
    return;
  if (review.getSnapshot().mutationId) {
    try {
      await review.close();
    } catch (reason) {
      failure(reason);
      return;
    }
  }
  if (!current() || runtime.picking) return;
  runtime.picking = true;
  if (runtime.approval) mediaPlayback.pauseAll();
  publish({ picking: true, error: null });
  let selected: AttachmentAsset[] = [];
  let delegated = false;
  let approve = false;
  let directSend: MediaDraftAttachments | null = null;
  let editAfterPicking: string | null = null;
  const pickedFor = runtime.approval;
  try {
    await sensitiveCacheWork.run(async () => {
      const admissionDraft = channel.getSnapshot();
      const before = admissionDraft.content;
      const count = before?.kind === 'attachments' ? before.items.length : 0;
      const sendAsFileAllowed =
        count === 0 || (before?.kind === 'attachments' && before.options?.presentation === 'file');
      if (count >= MAX_ATTACHMENT_ITEMS) throw new AttachmentDraftFailure('limit');
      selected = await (kind === 'photo'
        ? photos.pick(MAX_ATTACHMENT_ITEMS - count, current, {
            sendAsFileAllowed,
            ...(before?.kind === 'attachments' && count > 0
              ? {
                  existingOptions: before.options ?? {
                    presentation: 'media',
                    grouping: 'separate',
                  },
                }
              : {}),
          })
        : pickAttachmentAssets(kind, MAX_ATTACHMENT_ITEMS - count, current));
      if (!selected.length) return;
      if (!current()) return;
      const latest = channel.getSnapshot();
      if (latest.content && latest.content.kind !== 'attachments')
        throw new AttachmentDraftFailure('unavailable');
      const existing = latest.content?.kind === 'attachments' ? latest.content : null;
      const submission = kind === 'photo' ? photos.submission?.() : null;
      if (
        submission?.options &&
        (latest.value?.revision !== admissionDraft.value?.revision ||
          !['media', 'file'].includes(submission.options.presentation) ||
          !['separate', 'grouped'].includes(submission.options.grouping) ||
          (submission.options.presentation === 'file' &&
            (!sendAsFileAllowed ||
              ((!existing || !existing.items.length) &&
                (!submission.send || submission.requestEdit)) ||
              submission.options.grouping !== 'separate' ||
              (existing &&
                existing.items.length > 0 &&
                existing.options?.presentation !== 'file'))) ||
          (submission.options.grouping === 'grouped' &&
            (existing?.items.length ?? 0) + selected.length < 2) ||
          (existing &&
            existing.items.length > 0 &&
            (submission.options.presentation !== (existing.options?.presentation ?? 'media') ||
              submission.options.grouping !== (existing.options?.grouping ?? 'separate'))))
      )
        throw new AttachmentDraftFailure('unavailable');
      if ((existing?.items.length ?? 0) + selected.length > MAX_ATTACHMENT_ITEMS)
        throw new AttachmentDraftFailure('limit');
      const retained =
        existing?.items.reduce(
          (sum, item) =>
            sum +
            (runtime.previews.get(item.asset_id)?.size ??
              latest.value?.assets.find((asset) => asset.id === item.asset_id)?.size ??
              Infinity),
          0,
        ) ?? 0;
      if (
        selected.some((asset) => !Number.isSafeInteger(asset.size) || asset.size <= 0) ||
        selected.reduce((sum, asset) => sum + asset.size, retained) > MAX_ATTACHMENT_TOTAL_BYTES
      )
        throw new AttachmentDraftFailure('size');
      const additions = selected.map((asset) => ({ ...asset, id: randomUUID() }));
      const inputs: MediaDraftIngest[] = additions.map(({ id, uri, name, mime }) => ({
        id,
        uri,
        name,
        mime,
      }));
      for (const asset of additions)
        runtime.previews.set(asset.id, {
          ...asset,
          release: async () => {
            await store.driver.media?.releaseLocalFile(asset.uri);
            discardCacheCopy(asset.uri);
          },
        });
      delegated = true;
      const preferred = kind === 'photo' ? photos.preferredUri?.() : null;
      publish({
        previews: new Map(runtime.previews),
        selected:
          additions.find((asset) => asset.uri === preferred)?.id ?? additions[0]?.id ?? null,
      });
      const next: MediaDraftAttachments = {
        kind: 'attachments',
        reply_to: existing ? existing.reply_to : admission.current.replyTo,
        items: [
          ...(existing?.items ?? []),
          ...additions.map(({ id }, index) => ({
            asset_id: id,
            caption:
              submission?.captions?.[index] ?? (index === 0 ? (submission?.caption ?? '') : ''),
            mutation_id: id,
          })),
        ],
        ...((existing?.options ?? submission?.options)
          ? { options: existing?.options ?? submission!.options }
          : {}),
      };
      await channel.save(next, inputs);
      if (submission?.requestEdit) {
        editAfterPicking =
          additions.find((asset) => asset.uri === preferred)?.id ?? additions[0]?.id ?? null;
      }
      if (submission?.send) directSend = next;
      else approve = selected.some((asset) => /^(image|video)\//.test(asset.mime));
    });
  } catch (reason) {
    failure(reason);
  } finally {
    if (!delegated)
      for (const asset of selected) {
        try {
          await store.driver.media?.releaseLocalFile(asset.uri);
          discardCacheCopy(asset.uri);
        } catch (reason) {
          failure(reason);
        }
      }
    runtime.picking = false;
    publish({ picking: false });
    // The picker resolves only after the real native sheet removal. The
    // existing journal saves caption/selection before the same send path runs.
    // Background alone does not revoke an explicitly accepted Send; the
    // account, destination, selection and route must still be current below.
    if (directSend && runtime.approval === pickedFor) await send(directSend, pickedFor);
    else if (approve && runtime.approval === pickedFor) {
      openApproval(editAfterPicking ?? undefined);
      if (editAfterPicking && runtime.approval) {
        const presentation = runtime.approval;
        await edit(editAfterPicking, presentation);
      }
    }
  }
}
