import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState, Keyboard, View, Text } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { randomUUID } from 'expo-crypto';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { MediaDraftChannel } from './media-draft-channel';
import type { MediaDraftAttachments, MediaDraftIngest } from './media-draft-types';
import { AttachmentReview } from './attachment-review';
import type { AttachmentComposerField, AttachmentComposerOptions } from './attachment-composer';
import {
  AttachmentDraftFailure,
  MAX_ATTACHMENT_ITEMS,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
  type AttachmentDraftState,
} from './attachment-draft';
import { AttachmentDraftStrip } from './attachment-draft-strip';
import { AttachmentApproval, supportsMediaApproval } from './attachment-approval';
import { pickAttachmentAssets, type AttachmentPickKind } from './attachment-picker';
import { useAttachmentPhotoPicker } from './attachment-photo-picker';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import { mediaPlayback } from './playback-owner';

const emptySubscribe = () => () => {};
const emptySnapshot = () => null;
type Preview = AttachmentAsset & { release: () => Promise<void> };
type ApprovalPresentation = { closing: boolean };
const sameAttachmentSelection = (value: MediaDraftAttachments, observed: MediaDraftAttachments) =>
  value.reply_to === observed.reply_to &&
  (value.options?.presentation ?? 'media') === (observed.options?.presentation ?? 'media') &&
  (value.options?.grouping ?? 'separate') === (observed.options?.grouping ?? 'separate') &&
  value.items.length === observed.items.length &&
  value.items.every(
    (item, index) =>
      item.asset_id === observed.items[index]?.asset_id &&
      item.mutation_id === observed.items[index]?.mutation_id,
  );

/** Rust owns durable content; this route owns only replaceable preview leases. */
export function useDurableAttachmentComposer(
  options: AttachmentComposerOptions,
  channel: MediaDraftChannel | null,
) {
  const { store, snapshot } = useClient();
  const recentConfiguration = useMemo(
    () =>
      channel && store.driver.media?.recentAttachments
        ? { scope: channel.scope, port: store.driver.media.recentAttachments }
        : undefined,
    [channel, store],
  );
  const photos = useAttachmentPhotoPicker(recentConfiguration);
  const cancelPhotos = photos.cancel;
  const { colors } = useTheme();
  const state = useSyncExternalStore(
    channel?.subscribe ?? emptySubscribe,
    channel?.getSnapshot ?? emptySnapshot,
    channel?.getSnapshot ?? emptySnapshot,
  );
  const runtime = useMemo(
    () => ({
      channel,
      closed: false,
      focused: false,
      foreground: AppState.currentState !== 'background' && AppState.currentState !== 'inactive',
      picking: false,
      submitting: false,
      approval: null as ApprovalPresentation | null,
      previews: new Map<string, Preview>(),
    }),
    [channel],
  );
  const admission = useRef(options);
  useLayoutEffect(() => {
    admission.current = options;
  }, [options]);
  const current = useCallback(
    () =>
      !!channel &&
      !runtime.closed &&
      runtime.focused &&
      admission.current.spaceId === channel.scope.space_id &&
      (admission.current.threadRootId ?? null) === channel.scope.thread_root_id &&
      !admission.current.disabled &&
      !admission.current.suspended &&
      store.mediaDrafts.isCurrent(channel),
    [channel, runtime, store],
  );
  const review = useMemo(
    () =>
      new AttachmentReview({
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
              (item) =>
                item.mutation_id === request.mutationId && item.asset_id === request.assetId,
            ) &&
            latest.value?.assets.some(
              (asset) => asset.id === request.assetId && asset.sha256 === request.sha256,
            ),
          );
        },
        preview: (request, token) => {
          const port = store.driver.media?.drafts;
          if (!port?.preview || !port.cancelPreview)
            throw new AttachmentDraftFailure('unavailable');
          const latest = channel?.getSnapshot();
          const asset = latest?.value?.assets.find((value) => value.id === request.assetId);
          const editable =
            asset &&
            /^(image\/(jpeg|png|heic|heif)|video\/(mp4|quicktime|x-m4v))$/.test(asset.mime);
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
      }),
    [channel, current, store, runtime],
  );
  const reviewing = useSyncExternalStore(review.subscribe, review.getSnapshot, review.getSnapshot);
  const canReview = Boolean(
    store.driver.media?.drafts?.preview && store.driver.media?.drafts?.cancelPreview,
  );
  const [restoreEpoch, setRestoreEpoch] = useState(0);
  const [local, setLocal] = useState<{
    channel: MediaDraftChannel | null;
    picking: boolean;
    clearing: boolean;
    selected: string | null;
    approval: ApprovalPresentation | null;
    previews: ReadonlyMap<string, Preview>;
    error: string | null;
  }>({
    channel,
    picking: false,
    clearing: false,
    selected: null,
    approval: null,
    previews: new Map(),
    error: null,
  });
  const content = state?.content?.kind === 'attachments' ? state.content : null;
  const own = local.channel === channel ? local : null;
  const publish = useCallback(
    (patch: Partial<typeof local>) => {
      if (runtime.closed) return;
      setLocal((previous) => ({
        ...(previous.channel === channel
          ? previous
          : {
              channel,
              picking: false,
              clearing: false,
              selected: null,
              approval: null,
              previews: new Map(),
              error: null,
            }),
        ...patch,
        channel,
      }));
    },
    [runtime, channel],
  );
  const retireApproval = useCallback(() => {
    if (!runtime.approval) return;
    mediaPlayback.pauseAll();
    runtime.approval = null;
    publish({ approval: null });
  }, [runtime, publish]);
  useLayoutEffect(() => {
    if (!current()) retireApproval();
    review.refresh();
  }, [current, retireApproval, review, options]);
  useEffect(() => {
    const refresh = () => {
      if (!current()) {
        retireApproval();
        cancelPhotos();
      }
      review.refresh();
    };
    const close = async () => {
      cancelPhotos();
      retireApproval();
      await review.close();
    };
    const stopStore = store.subscribe(refresh);
    const stopDraft = channel?.subscribe(refresh);
    const stopDevice = store.registerDeviceCleanup(close);
    const stopSession = store.registerSessionCleanup(close);
    const background = AppState.addEventListener('change', (value) => {
      runtime.foreground = value === 'active';
      if (value !== 'active') {
        retireApproval();
        if (value === 'background') cancelPhotos();
        void review.close().catch(() => {});
      }
    });
    return () => {
      stopStore();
      stopDraft?.();
      background.remove();
      // A failed child cleanup remains registered for erasure/session retry.
      void close()
        .then(() => {
          stopDevice();
          stopSession();
        })
        .catch(() => {});
    };
  }, [review, channel, store, current, retireApproval, runtime, cancelPhotos]);
  useFocusEffect(
    useCallback(() => {
      runtime.focused = true;
      return () => {
        runtime.focused = false;
        cancelPhotos();
        retireApproval();
        void review.close().catch(() => {});
      };
    }, [runtime, review, retireApproval, cancelPhotos]),
  );
  useEffect(() => {
    runtime.closed = false;
    if (channel) void channel.load().catch(() => {});
    return () => {
      runtime.closed = true;
      runtime.approval = null;
      const release = async () => {
        await review.close();
        const results = await Promise.allSettled(
          [...runtime.previews.values()].map((preview) => preview.release()),
        );
        const failed = results.find((result) => result.status === 'rejected');
        if (failed?.status === 'rejected') throw failed.reason;
        runtime.previews.clear();
      };
      channel?.deferRelease(release);
    };
  }, [channel, runtime, review]);
  const assetKey = JSON.stringify(content?.items.map((item) => item.asset_id) ?? []);
  useEffect(() => {
    if (!channel) return;
    let canceled = false;
    const ids = content?.items.map((item) => item.asset_id) ?? [];
    for (const [id, preview] of runtime.previews) {
      if (ids.includes(id)) continue;
      runtime.previews.delete(id);
      channel.deferRelease(async () => {
        await review.close();
        await preview.release();
      });
    }
    publish({ previews: new Map(runtime.previews) });
    const restore = async () => {
      for (const id of ids) {
        if (runtime.previews.has(id)) continue;
        const file = await channel.materialize(id);
        if (canceled || runtime.closed || !store.mediaDrafts.isCurrent(channel)) {
          await channel.release(file.uri);
          continue;
        }
        runtime.previews.set(id, { ...file, release: () => channel.release(file.uri) });
        publish({ previews: new Map(runtime.previews) });
      }
    };
    void sensitiveCacheWork.run(restore).catch((reason: unknown) => {
      if (!canceled)
        publish({
          error: reason instanceof Error ? reason.message : t('media.attachmentDraftUnavailable'),
        });
    });
    return () => {
      canceled = true;
    };
    // Caption edits keep the same native previews; only ordered asset identity changes them.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [channel, assetKey, runtime, store, publish, restoreEpoch, review]);
  function failure(reason: unknown) {
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
  function openApproval(id?: string) {
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
  async function closeApproval() {
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
  async function pick(kind: AttachmentPickKind) {
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
          count === 0 ||
          (before?.kind === 'attachments' && before.options?.presentation === 'file');
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
  async function cancel() {
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
  async function remove(id: string) {
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
              ...currentContent,
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
  async function reorder(ids: readonly string[]) {
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
      await channel.save({ ...latest.content, items: ids.map((id) => byId.get(id)!) });
    } catch (reason) {
      failure(reason);
    }
  }
  async function edit(
    mutationId: string,
    presentation: ApprovalPresentation,
    finalCaption?: string,
  ) {
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
    if (before?.kind !== 'attachments' || latest.frozen || latest.sending || latest.reviewing)
      return;
    const index = before.items.findIndex((item) => item.mutation_id === mutationId);
    if (index < 0) return;
    if (
      finalCaption !== undefined &&
      (typeof finalCaption !== 'string' ||
        finalCaption.length > 4096 ||
        finalCaption.includes('\0'))
    )
      return;
    runtime.submitting = true;
    publish({ error: null });
    try {
      if (finalCaption !== undefined && before.items[index]!.caption !== finalCaption)
        await channel.save({
          ...before,
          items: before.items.map((item) =>
            item.mutation_id === mutationId ? { ...item, caption: finalCaption } : item,
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
  async function send(
    observed: MediaDraftAttachments,
    presentation: ApprovalPresentation | null,
    finalCaption?: { mutationId: string; text: string },
  ) {
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
            ...latest.content,
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
      const result = await sensitiveCacheWork.run(() => channel.send());
      if (result.entries.length) {
        retireApproval();
        if (!runtime.closed && store.mediaDrafts.isCurrent(channel)) sent?.();
        void store.refresh().catch(() => {});
      }
    } catch (reason) {
      failure(reason);
    } finally {
      runtime.submitting = false;
      publish({});
    }
  }
  const presentation = runtime.approval;
  // A native-review writer freeze is not a pending Send.
  const retry = Boolean(state?.frozen && !state.reviewing);
  const restoring = !!content?.items.some((item) => !own?.previews.has(item.asset_id));
  const busy = Boolean(
    runtime.submitting ||
    reviewing.mutationId ||
    presentation?.closing ||
    own?.picking ||
    own?.clearing ||
    state?.sending ||
    restoring ||
    (state?.phase === 'loading' && !state.error),
  );
  const scope = channel
    ? {
        mode: 'account' as const,
        identityId: channel.scope.expected_identity_id,
        deviceId: channel.scope.expected_device_id,
        spaceId: channel.scope.space_id,
        replyTo: content?.reply_to ?? channel.scope.thread_root_id,
        thread: Boolean(channel.scope.thread_root_id),
      }
    : null;
  const items =
    content?.items.map((item) => {
      const asset =
        own?.previews.get(item.asset_id) ??
        state?.value?.assets.find((asset) => asset.id === item.asset_id);
      return {
        uri: own?.previews.get(item.asset_id)?.uri ?? '',
        name: asset?.name ?? '',
        mime: asset?.mime ?? 'application/octet-stream',
        size: asset?.size ?? 0,
        mutationId: item.mutation_id,
        caption: item.caption,
        attempted: retry,
        accepted: null,
      };
    }) ?? [];
  const selectedIndex = Math.max(
    0,
    content?.items.findIndex((item) => item.asset_id === own?.selected) ?? 0,
  );
  const selected = items[selectedIndex];
  const field: AttachmentComposerField | undefined =
    selected && content && scope
      ? {
          scope,
          targetFrozen: true,
          value: selected.caption,
          position: selectedIndex + 1,
          total: items.length,
          editable:
            !options.disabled &&
            !options.suspended &&
            !reviewing.mutationId &&
            !state?.frozen &&
            !restoring,
          sending: busy,
          remaining: items.length,
          retry,
          onChange: (caption) => {
            if (
              !channel ||
              !current() ||
              // Photos anchors to this editable native field. While its overlay
              // owns interaction, even a retained callback cannot change the draft.
              runtime.picking ||
              state?.frozen ||
              runtime.approval !== presentation ||
              presentation?.closing
            )
              return;
            const next = channel.getSnapshot().content;
            if (next?.kind !== 'attachments') return;
            void channel
              .save({
                ...next,
                items: next.items.map((item) =>
                  item.mutation_id === selected.mutationId ? { ...item, caption } : item,
                ),
              })
              .catch(failure);
          },
          onSend: (finalCaption) => {
            void send(
              content,
              presentation,
              finalCaption === undefined
                ? undefined
                : { mutationId: selected.mutationId, text: finalCaption },
            );
          },
        }
      : undefined;
  const reviewFailed = !own?.error && !state?.error && Boolean(reviewing.error);
  const error =
    own?.error ??
    state?.error?.message ??
    (reviewFailed ? t('media.attachmentPreviewFailed') : null);
  const strip: AttachmentDraftState | null =
    scope && content
      ? {
          scope,
          items,
          targetFrozen: true,
          phase: state?.sending ? 'sending' : own?.picking || restoring ? 'picking' : 'ready',
          error: null,
        }
      : null;
  return {
    active: Boolean(content?.items.length),
    busy,
    field,
    pick,
    canAdd: Boolean(
      !reviewing.mutationId &&
      state?.phase === 'ready' &&
      !state.frozen &&
      (!state.content || state.content.kind === 'attachments') &&
      items.length < MAX_ATTACHMENT_ITEMS,
    ),
    review: (
      <>
        {!(strip && field && presentation && own?.approval === presentation) ? photos.view : null}
        {strip || (!state?.content && error) ? (
          <View style={{ gap: 8 }}>
            {strip ? (
              <AttachmentDraftStrip
                state={strip}
                selectedId={selected?.mutationId ?? null}
                busy={Boolean(
                  reviewing.mutationId ||
                  own?.picking ||
                  own?.clearing ||
                  state?.sending ||
                  (restoring && !error),
                )}
                error={error}
                onPreview={
                  canReview
                    ? (id) => {
                        const item = items.find((item) => item.mutationId === id);
                        if (supportsMediaApproval && item && /^(image|video)\//.test(item.mime))
                          openApproval(id);
                        else void review.open(id);
                      }
                    : undefined
                }
                onSelect={(id) =>
                  publish({
                    selected:
                      content?.items.find((item) => item.mutation_id === id)?.asset_id ?? null,
                  })
                }
                onRemove={(id) => {
                  const item = content?.items.find((item) => item.mutation_id === id);
                  if (item) void remove(item.asset_id);
                }}
                onCancel={() => {
                  void cancel();
                }}
              />
            ) : (
              <Text accessibilityRole="alert" style={{ color: colors.danger }}>
                {error}
              </Text>
            )}
            {strip && field && presentation && own?.approval === presentation ? (
              <AttachmentApproval
                grouped={content?.options?.grouping === 'grouped'}
                libraryPicker={photos.view}
                testID="attachment-approval"
                state={strip}
                field={field}
                selectedId={selected?.mutationId ?? null}
                recipient={
                  snapshot?.conversations.find((chat) => chat.id === channel?.scope.space_id)
                    ?.title ?? t('media.attachmentConversation')
                }
                busy={busy}
                editing={Boolean(reviewing.mutationId)}
                suspended={Boolean(own?.picking || reviewing.mutationId)}
                canAdd={!state?.frozen && !busy && items.length < MAX_ATTACHMENT_ITEMS}
                canEdit={Boolean(
                  canReview &&
                  store.driver.media?.drafts?.edit &&
                  !state?.frozen &&
                  selected &&
                  /^(image\/(jpeg|png|heic|heif)|video\/(mp4|quicktime|x-m4v))$/.test(
                    selected.mime,
                  ),
                )}
                error={error}
                isCurrent={() =>
                  Boolean(
                    current() &&
                    runtime.foreground &&
                    runtime.approval === presentation &&
                    !presentation.closing &&
                    !runtime.picking &&
                    !review.getSnapshot().mutationId &&
                    !channel?.getSnapshot().sending,
                  )
                }
                onClose={() => {
                  void closeApproval();
                }}
                onSelect={(id) => {
                  if (
                    !current() ||
                    runtime.approval !== presentation ||
                    presentation.closing ||
                    busy
                  )
                    return;
                  mediaPlayback.pauseAll();
                  publish({
                    selected:
                      content?.items.find((item) => item.mutation_id === id)?.asset_id ?? null,
                  });
                }}
                onEdit={(finalCaption) => {
                  if (runtime.approval === presentation && !presentation.closing && selected)
                    void edit(selected.mutationId, presentation, finalCaption);
                }}
                onReorder={(ids) => {
                  if (runtime.approval === presentation && !presentation.closing) void reorder(ids);
                }}
                onAdd={(kind) => {
                  if (runtime.approval === presentation && !presentation.closing) void pick(kind);
                }}
                onRemove={(id) => {
                  if (runtime.approval !== presentation || presentation.closing) return;
                  const item = content?.items.find((item) => item.mutation_id === id);
                  if (item) void remove(item.asset_id);
                }}
                onRetry={() => {
                  if (runtime.approval !== presentation || presentation.closing) return;
                  publish({ error: null });
                  void channel
                    ?.retry()
                    .then(() => {
                      if (!runtime.closed) setRestoreEpoch((value) => value + 1);
                    })
                    .catch(failure);
                }}
              />
            ) : null}
            {error && channel ? (
              <NativeButton
                testID="retry-media-draft"
                secondary
                label={t('chat.tryAgain2')}
                onPress={() => {
                  if (reviewFailed && selected) {
                    void review.open(selected.mutationId);
                    return;
                  }
                  publish({ error: null });
                  void channel
                    .retry()
                    .then(() => {
                      if (!runtime.closed) setRestoreEpoch((value) => value + 1);
                    })
                    .catch(failure);
                }}
              />
            ) : null}
          </View>
        ) : null}
      </>
    ),
  };
}
