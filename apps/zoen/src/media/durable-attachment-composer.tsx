import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { View, Text } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { randomUUID } from 'expo-crypto';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { MediaDraftChannel } from './media-draft-channel';
import type { MediaDraftAttachments, MediaDraftIngest } from './media-draft-types';
import type { AttachmentComposerField, AttachmentComposerOptions } from './attachment-composer';
import {
  AttachmentDraftFailure,
  MAX_ATTACHMENT_ITEMS,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
  type AttachmentDraftState,
} from './attachment-draft';
import { AttachmentDraftStrip } from './attachment-draft-strip';
import { pickAttachmentAssets, type AttachmentPickKind } from './attachment-picker';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';

const emptySubscribe = () => () => {};
const emptySnapshot = () => null;
type Preview = AttachmentAsset & { release: () => Promise<void> };

/** Rust owns durable content; this route owns only replaceable preview leases. */
export function useDurableAttachmentComposer(
  options: AttachmentComposerOptions,
  channel: MediaDraftChannel | null,
) {
  const { store } = useClient();
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
      picking: false,
      previews: new Map<string, Preview>(),
    }),
    [channel],
  );
  const admission = useRef(options);
  useLayoutEffect(() => {
    admission.current = options;
  }, [options]);
  const [restoreEpoch, setRestoreEpoch] = useState(0);
  const [local, setLocal] = useState<{
    channel: MediaDraftChannel | null;
    picking: boolean;
    clearing: boolean;
    selected: string | null;
    previews: ReadonlyMap<string, Preview>;
    error: string | null;
  }>({
    channel,
    picking: false,
    clearing: false,
    selected: null,
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
              previews: new Map(),
              error: null,
            }),
        ...patch,
        channel,
      }));
    },
    [runtime, channel],
  );
  useFocusEffect(
    useCallback(() => {
      runtime.focused = true;
      return () => {
        runtime.focused = false;
      };
    }, [runtime]),
  );
  useEffect(() => {
    runtime.closed = false;
    if (channel) void channel.load().catch(() => {});
    return () => {
      runtime.closed = true;
      const release = async () => {
        const results = await Promise.allSettled(
          [...runtime.previews.values()].map((preview) => preview.release()),
        );
        const failed = results.find((result) => result.status === 'rejected');
        if (failed?.status === 'rejected') throw failed.reason;
        runtime.previews.clear();
      };
      channel?.deferRelease(release);
    };
  }, [channel, runtime]);
  const assetKey = JSON.stringify(content?.items.map((item) => item.asset_id) ?? []);
  useEffect(() => {
    if (!channel) return;
    let canceled = false;
    const ids = content?.items.map((item) => item.asset_id) ?? [];
    for (const [id, preview] of runtime.previews) {
      if (ids.includes(id)) continue;
      runtime.previews.delete(id);
      channel.deferRelease(preview.release);
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
  }, [channel, assetKey, runtime, store, publish, restoreEpoch]);
  function current() {
    return (
      !!channel &&
      !runtime.closed &&
      runtime.focused &&
      !admission.current.disabled &&
      !admission.current.suspended &&
      store.mediaDrafts.isCurrent(channel)
    );
  }
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
    runtime.picking = true;
    publish({ picking: true, error: null });
    let selected: AttachmentAsset[] = [];
    let delegated = false;
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
    }
  }
  async function cancel() {
    if (!channel || !current() || state?.sending || runtime.picking || own?.clearing) return;
    publish({ clearing: true, error: null });
    try {
      await channel.clear();
    } catch (reason) {
      failure(reason);
    } finally {
      publish({ clearing: false });
    }
  }
  async function remove(id: string) {
    if (!channel || !current()) return;
    const latest = channel.getSnapshot();
    const currentContent = latest.content;
    if (latest.frozen || currentContent?.kind !== 'attachments') return;
    const items = currentContent.items.filter((item) => item.asset_id !== id);
    try {
      await channel.save(items.length ? { ...currentContent, items } : null);
    } catch (reason) {
      failure(reason);
    }
  }
  async function send() {
    if (!channel || !current() || runtime.picking) return;
    const latest = channel.getSnapshot();
    if (latest.content?.kind !== 'attachments' || latest.sending) return;
    const sent = admission.current.onSent;
    publish({ error: null });
    try {
      const result = await sensitiveCacheWork.run(() => channel.send());
      if (result.entries.length) {
        if (!runtime.closed && store.mediaDrafts.isCurrent(channel)) sent?.();
        void store.refresh().catch(() => {});
      }
    } catch (reason) {
      failure(reason);
    }
  }
  const restoring = !!content?.items.some((item) => !own?.previews.has(item.asset_id));
  const busy = Boolean(
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
        attempted: Boolean(state?.frozen),
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
            !state?.frozen &&
            !own?.picking &&
            !restoring,
          sending: busy,
          remaining: items.length,
          retry: Boolean(state?.frozen),
          onChange: (caption) => {
            if (!channel || !current() || state?.frozen) return;
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
            void send();
          },
        }
      : undefined;
  const error = own?.error ?? state?.error?.message ?? null;
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
                own?.picking || own?.clearing || state?.sending || (restoring && !error),
              )}
              error={error}
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
          {error && channel ? (
            <NativeButton
              testID="retry-media-draft"
              secondary
              label={t('chat.tryAgain2')}
              onPress={() => {
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
