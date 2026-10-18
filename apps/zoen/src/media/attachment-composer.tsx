import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { Platform } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { File } from 'expo-file-system';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import { randomUUID } from 'expo-crypto';
import { useClient } from '../core/provider';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { t } from '../i18n/index.ts';
import { AttachmentDraftStrip } from './attachment-draft-strip';
import {
  AttachmentDraft,
  AttachmentDraftFailure,
  MAX_ATTACHMENT_ITEMS,
  type AttachmentAsset,
  type AttachmentDraftScope,
} from './attachment-draft';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';

const subscribeEmpty = () => () => {};
const emptySnapshot = () => null;
function scopeAvailable(
  client: Parameters<typeof homeOwnerFor>[0],
  scope: AttachmentDraftScope,
  spaceId: string,
) {
  const session = homeOwnerFor(client);
  return (
    session?.mode === scope.mode &&
    session.identityId === scope.identityId &&
    session.deviceId === scope.deviceId &&
    scope.spaceId === spaceId &&
    homeSpacesFor(client).has(scope.spaceId)
  );
}
export type AttachmentComposerField = {
  scope: AttachmentDraftScope;
  targetFrozen: boolean;
  value: string;
  position: number;
  total: number;
  editable: boolean;
  sending: boolean;
  remaining: number;
  retry: boolean;
  onChange: (value: string) => void;
  onSend: () => void;
};

function draftFailure(reason: unknown): string {
  if (reason instanceof AttachmentDraftFailure) {
    if (reason.kind === 'caption') return t('media.attachmentCaptionLimit');
    if (reason.kind === 'limit') return t('media.attachmentDraftLimit');
    if (reason.kind === 'size') return t('media.attachmentTooLarge');
    if (reason.kind === 'busy') return t('media.attachmentPreparing');
    if (reason.kind === 'cleanup') return t('media.attachmentCleanupFailed');
    if (reason.kind === 'unavailable') return t('media.attachmentDraftUnavailable');
    if (reason.kind === 'invalid') return t('media.attachmentMissingSize');
    if (reason.kind === 'send') return t('media.attachmentFailed');
    if (reason.cause instanceof Error) return reason.cause.message;
  }
  return reason instanceof Error ? reason.message : t('media.theMediaCouldNotBeSelected');
}
async function discard(uri: string) {
  if (Platform.OS === 'web') {
    if (uri.startsWith('blob:')) URL.revokeObjectURL(uri);
  } else discardCacheCopy(uri);
}
/** Staged SDK files stay owned until the batch accepts them or cleanup succeeds. */
export function useAttachmentComposer({
  spaceId,
  replyTo,
  thread,
  disabled,
  suspended,
  onSent,
}: {
  spaceId: string;
  replyTo: string | null;
  thread: boolean;
  disabled: boolean;
  suspended: boolean;
  onSent?: () => void;
}) {
  const client = useClient();
  const { store } = client;
  const [owner, setOwner] = useState<AttachmentDraft | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);
  const ownerNow = useRef<AttachmentDraft | null>(null);
  const mounted = useRef(true);
  const focused = useRef(false);
  const admission = useRef({ spaceId, disabled, suspended, replyTo, thread, onSent });
  const owners = useRef(new Map<AttachmentDraft, () => void>());
  const acceptedCallbacks = useRef(new Map<AttachmentDraft, (() => void) | undefined>());
  const state = useSyncExternalStore(
    owner?.subscribe ?? subscribeEmpty,
    owner?.getSnapshot ?? emptySnapshot,
    owner?.getSnapshot ?? emptySnapshot,
  );
  useLayoutEffect(() => {
    admission.current = { spaceId, disabled, suspended, replyTo, thread, onSent };
  }, [spaceId, disabled, suspended, replyTo, thread, onSent]);
  const closeOwner = useCallback(async (draft: AttachmentDraft) => {
    await sensitiveCacheWork.track(draft.close());
    owners.current.get(draft)?.();
    owners.current.delete(draft);
    acceptedCallbacks.current.delete(draft);
  }, []);
  useEffect(() => {
    mounted.current = true;
    const retained = owners.current;
    return () => {
      mounted.current = false;
      for (const draft of retained.keys()) void closeOwner(draft).catch(() => {});
    };
  }, [closeOwner]);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
      };
    }, []),
  );
  useEffect(() => {
    owner?.retarget(replyTo, thread);
  }, [owner, replyTo, thread]);

  const available = useCallback(
    (scope: AttachmentDraftScope) =>
      scopeAvailable(store.getSnapshot(), scope, admission.current.spaceId),
    [store],
  );
  useEffect(() => {
    if (owner && !available(owner.getSnapshot().scope)) {
      // A failed cleanup remains registered with device erasure for a retry.
      void closeOwner(owner).catch(() => {});
    }
  }, [
    owner,
    closeOwner,
    available,
    spaceId,
    client.phase,
    client.mode,
    client.erasePending,
    client.snapshot,
  ]);
  function current(scope: AttachmentDraftScope) {
    return (
      mounted.current &&
      focused.current &&
      !admission.current.disabled &&
      !admission.current.suspended &&
      available(scope)
    );
  }
  function ensureOwner(): AttachmentDraft {
    const previous = ownerNow.current;
    if (
      previous &&
      available(previous.getSnapshot().scope) &&
      previous.getSnapshot().phase !== 'closed'
    )
      return previous;
    if (previous) void closeOwner(previous).catch(() => {});
    const client = store.getSnapshot();
    const scope: AttachmentDraftScope = {
      mode: client.mode,
      identityId: client.snapshot?.account?.identity_id ?? client.snapshot?.me?.id ?? '',
      deviceId: client.snapshot?.account?.device_id ?? (client.mode === 'demo' ? 'demo' : ''),
      spaceId,
      replyTo: admission.current.replyTo,
      thread: admission.current.thread,
    };
    if (!current(scope)) throw new AttachmentDraftFailure('unavailable');
    const media = store.driver.media;
    if (!media) throw new AttachmentDraftFailure('unavailable');
    const draft = new AttachmentDraft(scope, {
      current,
      mutationId: randomUUID,
      discard,
      send: (scope, item) =>
        media.sendFile({
          ...item,
          spaceId: scope.spaceId,
          replyTo: scope.replyTo,
          thread: scope.thread,
        }),
    });
    owners.current.set(
      draft,
      store.registerDeviceCleanup(() => closeOwner(draft)),
    );
    ownerNow.current = draft;
    setOwner(draft);
    return draft;
  }
  function forget(draft: AttachmentDraft) {
    if (ownerNow.current !== draft) return;
    ownerNow.current = null;
    if (mounted.current) {
      setOwner(null);
      setSelectedId(null);
    }
  }
  async function cancel() {
    const draft = ownerNow.current;
    if (
      !draft ||
      draft.getSnapshot().phase === 'sending' ||
      draft.getSnapshot().phase === 'picking' ||
      closing
    )
      return;
    setClosing(true);
    try {
      await closeOwner(draft);
      forget(draft);
      if (mounted.current) setFailure(null);
    } catch (reason) {
      if (mounted.current) setFailure(draftFailure(reason));
    } finally {
      if (mounted.current) setClosing(false);
    }
  }
  async function pick(kind: 'document' | 'photo' | 'camera') {
    let draft: AttachmentDraft | null = null;
    try {
      draft = ensureOwner();
      const owner = draft;
      if (owner.getSnapshot().items.length >= MAX_ATTACHMENT_ITEMS)
        throw new AttachmentDraftFailure('limit');
      setFailure(null);
      await sensitiveCacheWork.run(() =>
        owner.pick(async () => {
          let assets: { uri: string; name: string; mime: string; size?: number }[] = [];
          if (kind === 'document') {
            const result = await DocumentPicker.getDocumentAsync({
              copyToCacheDirectory: true,
              multiple: Platform.OS === 'ios',
              type: '*/*',
              base64: false,
            });
            if (!result.canceled)
              assets = result.assets.map((file) => ({
                uri: file.uri,
                name: file.name,
                mime: file.mimeType || 'application/octet-stream',
                size: file.size,
              }));
          } else {
            if (kind === 'camera') {
              const permission = await ImagePicker.requestCameraPermissionsAsync();
              if (!current(owner.getSnapshot().scope)) return [];
              if (!permission.granted)
                throw new Error(t('media.youCanAllowAccessInDeviceSettingsTo'));
            }
            const result =
              kind === 'camera'
                ? await ImagePicker.launchCameraAsync({
                    mediaTypes: ['images'],
                    quality: 1,
                    exif: false,
                  })
                : await ImagePicker.launchImageLibraryAsync({
                    mediaTypes: ['images', 'videos'],
                    quality: 1,
                    allowsMultipleSelection: Platform.OS === 'ios',
                    selectionLimit: MAX_ATTACHMENT_ITEMS - owner.getSnapshot().items.length,
                    orderedSelection: true,
                    exif: false,
                  });
            if (!result.canceled)
              assets = result.assets.map((asset) => ({
                uri: asset.uri,
                name: asset.fileName ?? (asset.type === 'video' ? 'video.mov' : 'photo.jpg'),
                mime: asset.mimeType ?? (asset.type === 'video' ? 'video/quicktime' : 'image/jpeg'),
                size: asset.fileSize,
              }));
          }
          // Even an unreadable selection enters the owner's cleanup set. Invalid
          // sizes reject the batch after it takes ownership of every returned URI.
          return assets.map((asset): AttachmentAsset => {
            let size = NaN;
            try {
              if (Platform.OS === 'web') size = asset.size ?? NaN;
              else {
                const file = new File(asset.uri);
                if (file.exists) size = file.size;
              }
            } catch {
              /* The owner rejects this asset and retains cleanup retries. */
            }
            return { ...asset, size };
          });
        }),
      );
      if (!owner.getSnapshot().items.length && !owner.getSnapshot().error) {
        await closeOwner(owner);
        forget(owner);
      }
    } catch (reason) {
      if (mounted.current && focused.current && (!draft || ownerNow.current === draft))
        setFailure(draftFailure(reason));
    }
  }
  const busy = closing || state?.phase === 'picking' || state?.phase === 'sending';
  const selected =
    state?.items.find((item) => item.mutationId === selectedId) ??
    state?.items.find((item) => !item.accepted) ??
    state?.items[0];
  const remaining = state?.items.filter((item) => !item.accepted).length ?? 0;
  async function send() {
    const draft = ownerNow.current;
    if (!draft || busy) return;
    if (!draft.getSnapshot().items.some((item) => item.attempted))
      acceptedCallbacks.current.set(draft, admission.current.onSent);
    setFailure(null);
    try {
      const result = await sensitiveCacheWork.run(() => draft.send());
      if (result.accepted.length) {
        const callback = acceptedCallbacks.current.get(draft);
        acceptedCallbacks.current.set(draft, undefined);
        if (result.current) callback?.();
        void store.refresh().catch(() => {});
      }
      if (result.complete) {
        await closeOwner(draft);
        forget(draft);
      }
    } catch (reason) {
      if (mounted.current && ownerNow.current === draft) setFailure(draftFailure(reason));
    }
  }
  const error = failure ?? (state?.error ? draftFailure(state.error) : null);
  const visible = Boolean(state && scopeAvailable(client, state.scope, spaceId));
  const field: AttachmentComposerField | undefined =
    selected && state && visible
      ? {
          scope: state.scope,
          targetFrozen: state.targetFrozen,
          value: selected.caption,
          position: state.items.indexOf(selected) + 1,
          total: state.items.length,
          editable:
            Platform.OS === 'ios' && !disabled && !suspended && !busy && !selected.attempted,
          sending: Boolean(busy),
          remaining,
          retry: state.items.some((item) => item.attempted && !item.accepted),
          onChange: (text) => {
            owner?.setCaption(selected.mutationId, text);
          },
          onSend: () => {
            void send();
          },
        }
      : undefined;
  return {
    active: Boolean(state?.items.length && visible),
    busy: Boolean(busy),
    field,
    pick,
    canAdd: (state?.items.length ?? 0) < MAX_ATTACHMENT_ITEMS,
    review:
      state && visible && (state.items.length || error || state.phase === 'picking') ? (
        <AttachmentDraftStrip
          state={state}
          selectedId={selected?.mutationId ?? null}
          busy={Boolean(busy)}
          error={error}
          onSelect={setSelectedId}
          onRemove={(id) => {
            void owner
              ?.remove(id)
              .then(async () => {
                if (owner && !owner.getSnapshot().items.length) {
                  await closeOwner(owner);
                  forget(owner);
                }
              })
              .catch((reason: unknown) => {
                if (mounted.current) setFailure(draftFailure(reason));
              });
          }}
          onCancel={() => {
            void cancel();
          }}
        />
      ) : null,
  };
}
