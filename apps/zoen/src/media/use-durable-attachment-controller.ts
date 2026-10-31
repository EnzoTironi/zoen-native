import { router, useFocusEffect } from 'expo-router';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState } from 'react-native';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { ConversationAttachmentCategory } from './attachment-categories';
import { conversationAttachmentCategories } from './attachment-categories';
import type { AttachmentComposerField, AttachmentComposerOptions } from './attachment-composer';
import { useAttachmentPhotoPicker } from './attachment-photo-picker';
import { type AttachmentPickKind } from './attachment-picker';
import { sensitiveCacheWork } from './cache-work';
import * as actions from './durable-attachment-commands';
import * as pickOperations from './durable-attachment-pick';
import { createDurableAttachmentReview } from './durable-attachment-review';
import type { ApprovalPresentation, Preview } from './durable-attachment-types';
import type { MediaDraftChannel } from './media-draft-channel';
import type { MediaDraftAttachments } from './media-draft-types';
import { mediaPlayback } from './playback-owner';

const emptySubscribe = () => () => {};
const emptySnapshot = () => null;
export function useDurableAttachmentController(
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
  const state = useSyncExternalStore(
    channel?.subscribe ?? emptySubscribe,
    channel?.getSnapshot ?? emptySnapshot,
    channel?.getSnapshot ?? emptySnapshot,
  );
  const routesAvailable =
    state?.phase === 'ready' && !state.frozen && !state.sending && !state.saving && !state.content;
  const richReply = Boolean(options.replyQuote || options.sharedReply);
  const routes = useMemo(
    () =>
      channel && routesAvailable
        ? {
            categories: conversationAttachmentCategories.filter(
              (kind) =>
                (!richReply || kind !== 'poll') &&
                (kind !== 'poll' || channel.scope.thread_root_id === null),
            ),
            open: (kind: ConversationAttachmentCategory, pickerCurrent: () => boolean) => {
              if (
                (richReply || admission.current.replyQuote || admission.current.sharedReply) &&
                kind === 'poll'
              )
                return;
              if (!pickerCurrent() || !store.mediaDrafts.isCurrent(channel)) return;
              const latest = channel.getSnapshot();
              if (
                latest.phase !== 'ready' ||
                latest.frozen ||
                latest.sending ||
                latest.saving ||
                latest.content
              )
                return;
              router.push({
                pathname: '/chat-attachment/[id]',
                params: {
                  id: channel.scope.space_id,
                  kind,
                  thread: channel.scope.thread_root_id ?? '',
                  identity: channel.scope.expected_identity_id,
                  device: channel.scope.expected_device_id,
                  epoch: String(store.sessionEpoch),
                },
              });
            },
          }
        : undefined,
    [channel, store, routesAvailable, richReply],
  );
  const photos = useAttachmentPhotoPicker(recentConfiguration, routes);
  const cancelPhotos = photos.cancel;
  const { colors } = useTheme();
  const runtime = useMemo(
    () => ({
      channel,
      closed: false,
      focused: false,
      foreground: AppState.currentState !== 'background' && AppState.currentState !== 'inactive',
      picking: false,
      pickerAdmission: null as object | null,
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
      createDurableAttachmentReview({
        channel,
        current,
        store,
        runtime,
        setLocal: (change) => setLocal(change),
      }),
    [channel, current, store, runtime],
  );
  const reviewing = useSyncExternalStore(review.subscribe, review.getSnapshot, review.getSnapshot);
  const canReview = Boolean(
    store.driver.media?.drafts?.preview && store.driver.media?.drafts?.cancelPreview,
  );
  const [restoreEpoch, setRestoreEpoch] = useState(0);
  const [pickerAnchor, setPickerAnchor] = useState<{
    channel: MediaDraftChannel | null;
    field: AttachmentComposerField | undefined;
  } | null>(null);
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
    return actions.failure({ publish }, reason);
  }
  function openApproval(id?: string) {
    return actions.openApproval({ channel, current, runtime, publish }, id);
  }
  function closeApproval() {
    return actions.closeApproval({ runtime, publish, review, retireApproval, failure });
  }
  async function pick(kind: AttachmentPickKind, field?: AttachmentComposerField) {
    // Admission also covers review drain before picking starts. A duplicate
    // callback cannot release the original native field's retained projection.
    if (!current() || runtime.pickerAdmission) return;
    const token = { channel, field };
    runtime.pickerAdmission = token;
    setPickerAnchor(token);
    try {
      await pickOperations.pick(
        {
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
        },
        kind,
      );
    } finally {
      if (runtime.pickerAdmission === token) {
        runtime.pickerAdmission = null;
        setPickerAnchor((current) => (current === token ? null : current));
      }
    }
  }
  function cancel() {
    return actions.cancel({
      channel,
      current,
      state,
      runtime,
      own,
      review,
      failure,
      publish,
      retireApproval,
    });
  }
  function remove(id: string) {
    return actions.remove({ channel, current, review, failure, retireApproval }, id);
  }
  function reorder(ids: readonly string[]) {
    return actions.reorder({ channel, current, runtime, review, failure }, ids);
  }
  function edit(mutationId: string, presentation: ApprovalPresentation, finalCaption?: string) {
    return actions.edit(
      { channel, current, runtime, review, publish, failure },
      mutationId,
      presentation,
      finalCaption,
    );
  }
  function send(
    observed: MediaDraftAttachments,
    presentation: ApprovalPresentation | null,
    finalCaption?: { mutationId: string; text: string },
  ) {
    return actions.send(
      { channel, current, runtime, review, admission, publish, retireApproval, store, failure },
      observed,
      presentation,
      finalCaption,
    );
  }
  return {
    runtime,
    state,
    content,
    own,
    reviewing,
    channel,
    options,
    current,
    failure,
    send,
    pick,
    pickerAnchor,
    photos,
    canReview,
    openApproval,
    review,
    publish,
    remove,
    cancel,
    colors,
    snapshot,
    store,
    closeApproval,
    edit,
    reorder,
    setRestoreEpoch,
  };
}
