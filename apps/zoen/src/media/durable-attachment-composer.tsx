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
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import { mediaPlayback } from './playback-owner';

const emptySubscribe = () => () => {};
const emptySnapshot = () => null;
type Preview = AttachmentAsset & { release: () => Promise<void> };
type ApprovalPresentation = { closing: boolean };

/** Rust owns durable content; this route owns only replaceable preview leases. */
export function useDurableAttachmentComposer(
  options: AttachmentComposerOptions,
  channel: MediaDraftChannel | null,
) {
  const { store, snapshot } = useClient();
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
      if (!current()) retireApproval();
      review.refresh();
    };
    const close = async () => {
      retireApproval();
      await review.close();
    };
    const stopStore = store.subscribe(refresh);
    const stopDraft = channel?.subscribe(refresh);
    const stopDevice = store.registerDeviceCleanup(close);
    const stopSession = store.registerSessionCleanup(close);
    const background = AppState.addEventListener('change', (value) => {
      runtime.foreground = value === 'active';
      if (value !== 'active') void close().catch(() => {});
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
  }, [review, channel, store, current, retireApproval, runtime]);
  useFocusEffect(
    useCallback(() => {
      runtime.focused = true;
      return () => {
        runtime.focused = false;
        retireApproval();
        void review.close().catch(() => {});
      };
    }, [runtime, review, retireApproval]),
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
    const pickedFor = runtime.approval;
    try {
      await sensitiveCacheWork.run(async () => {
        const before = channel.getSnapshot().content;
        const count = before?.kind === 'attachments' ? before.items.length : 0;
        if (count >= MAX_ATTACHMENT_ITEMS) throw new AttachmentDraftFailure('limit');
        selected = await pickAttachmentAssets(kind, MAX_ATTACHMENT_ITEMS - count, current);
        if (!selected.length) return;
        if (!current()) return;
        const latest = channel.getSnapshot();
        if (latest.content && latest.content.kind !== 'attachments')
          throw new AttachmentDraftFailure('unavailable');
        const existing = latest.content?.kind === 'attachments' ? latest.content : null;
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
            release: async () => discardCacheCopy(asset.uri),
          });
        delegated = true;
        publish({ previews: new Map(runtime.previews), selected: additions[0]?.id ?? null });
        const next: MediaDraftAttachments = {
          kind: 'attachments',
          reply_to: existing ? existing.reply_to : admission.current.replyTo,
          items: [
            ...(existing?.items ?? []),
            ...additions.map(({ id }) => ({ asset_id: id, caption: '', mutation_id: id })),
          ],
        };
        await channel.save(next, inputs);
        approve = selected.some((asset) => /^(image|video)\//.test(asset.mime));
      });
    } catch (reason) {
      failure(reason);
    } finally {
      if (!delegated)
        for (const asset of selected) {
          try {
            discardCacheCopy(asset.uri);
          } catch (reason) {
            failure(reason);
          }
        }
      runtime.picking = false;
      publish({ picking: false });
      if (approve && runtime.approval === pickedFor) openApproval();
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
      await channel.save(items.length ? { ...currentContent, items } : null);
      if (!items.length) retireApproval();
    } catch (reason) {
      failure(reason);
    }
  }
  async function send(observed: MediaDraftAttachments, presentation: ApprovalPresentation | null) {
    // A stale Send action must not dismiss review and queue delivery behind it.
    if (
      !channel ||
      !current() ||
      runtime.picking ||
      review.getSnapshot().mutationId ||
      runtime.approval !== presentation ||
      presentation?.closing
    )
      return;
    const latest = channel.getSnapshot();
    if (latest.content?.kind !== 'attachments' || latest.sending || latest.reviewing) return;
    // Caption saves can drain, but this action belongs only to its rendered selection.
    if (
      latest.content.reply_to !== observed.reply_to ||
      latest.content.items.length !== observed.items.length ||
      latest.content.items.some(
        (item, index) =>
          item.asset_id !== observed.items[index]?.asset_id ||
          item.mutation_id !== observed.items[index]?.mutation_id,
      )
    )
      return;
    const sent = admission.current.onSent;
    publish({ error: null });
    try {
      const result = await sensitiveCacheWork.run(() => channel.send());
      if (result.entries.length) {
        retireApproval();
        if (!runtime.closed && store.mediaDrafts.isCurrent(channel)) sent?.();
        void store.refresh().catch(() => {});
      }
    } catch (reason) {
      failure(reason);
    }
  }
  const presentation = runtime.approval;
  // A native-review writer freeze is not a pending Send.
  const retry = Boolean(state?.frozen && !state.reviewing);
  const restoring = !!content?.items.some((item) => !own?.previews.has(item.asset_id));
  const busy = Boolean(
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
            !own?.picking &&
            !restoring,
          sending: busy,
          remaining: items.length,
          retry,
          onChange: (caption) => {
            if (
              !channel ||
              !current() ||
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
          onSend: () => {
            void send(content, presentation);
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
    review:
      strip || (!state?.content && error) ? (
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
              canAdd={!state?.frozen && !busy && items.length < MAX_ATTACHMENT_ITEMS}
              canEdit={Boolean(
                canReview &&
                store.driver.media?.drafts?.edit &&
                !state?.frozen &&
                selected &&
                /^(image\/(jpeg|png|heic|heif)|video\/(mp4|quicktime|x-m4v))$/.test(selected.mime),
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
                if (!current() || runtime.approval !== presentation || presentation.closing || busy)
                  return;
                mediaPlayback.pauseAll();
                publish({
                  selected:
                    content?.items.find((item) => item.mutation_id === id)?.asset_id ?? null,
                });
              }}
              onEdit={() => {
                if (runtime.approval === presentation && !presentation.closing && selected)
                  void review.open(selected.mutationId);
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
      ) : null,
  };
}
