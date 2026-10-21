import { useLayoutEffect, useMemo, useRef, useSyncExternalStore } from 'react';
import { randomUUID } from 'expo-crypto';
import { Linking, useWindowDimensions } from 'react-native';
import { isLiquidGlassAvailable } from 'expo-glass-effect';
import * as Haptics from 'expo-haptics';
import { t } from '../i18n';
import type {
  AttachmentKeyboardSession,
  KeyboardPresentation,
} from './attachment-keyboard-session';
import type { AttachmentPhotoSession, PhotoPickerState } from './attachment-photo-session';
import { NativeAttachmentKeyboard, useKeyboardPanelBinding } from './keyboard-panel-native.ios';

export function AttachmentPhotoKeyboard({
  state,
  session,
  keyboard,
  request,
  onPreferredIndex,
}: {
  state: PhotoPickerState;
  session: AttachmentPhotoSession;
  keyboard: AttachmentKeyboardSession;
  request: KeyboardPresentation;
  onPreferredIndex(index: number | null): void;
}) {
  const owner = useSyncExternalStore(
    keyboard.subscribe,
    keyboard.getSnapshot,
    keyboard.getSnapshot,
  );
  const { width, height } = useWindowDimensions();
  const presentation = owner.presentation?.id === request.id ? owner.presentation : request;
  const renderOwner = useMemo(
    () => ({ state, request, owner, revision: randomUUID() }),
    [state, request, owner],
  );
  const revision = renderOwner.revision;
  const committed = useRef<object | null>(null);
  useLayoutEffect(() => {
    committed.current = renderOwner;
    return () => {
      if (committed.current === renderOwner) committed.current = null;
    };
  }, [renderOwner]);
  const admitted = () =>
    committed.current === renderOwner &&
    keyboard.isCurrent(request.id) &&
    session.isInteractive(state.token);
  const binding = useKeyboardPanelBinding(
    presentation,
    owner.enabled &&
      keyboard.owns(request.id) &&
      !presentation.closing &&
      state.visible &&
      (state.permission !== null || state.error !== null),
    presentation.closing || !state.visible,
  );
  const model = JSON.stringify({
    revision,
    columns: width > height ? 5 : 3,
    items: state.items.map((item) => ({
      id: item.id,
      type: item.mediaType,
      name: item.filename,
      duration: item.duration,
    })),
    selected: state.selected.map((item) => item.id),
    albums: state.albums,
    albumId: state.albumId,
    albumTitle:
      state.albums.find((album) => album.id === state.albumId)?.title ??
      t('media.photoLibraryRecents'),
    busy: state.busy,
    loading: state.loading,
    hasMore: state.hasMore,
    granted: state.permission?.granted ?? false,
    limited: state.permission?.accessPrivileges === 'limited',
    error: !!state.error,
    hasAudio: owner.hasAudio,
    glass: isLiquidGlassAvailable(),
    labels: {
      photos: t('media.photosAndVideos'),
      files: t('media.file2'),
      audio: t('media.voiceImportAudio'),
      camera: t('media.camera'),
      recents: t('media.photoLibraryRecents'),
      review: t('media.photoLibraryReview', { count: state.selected.length }),
      preparing: t('media.attachmentPreparing'),
      keyboard: t('composerKeyboard.keyboard'),
      manage: t('media.photoLibraryManageAccess'),
      permission: t('media.youCanAllowAccessInDeviceSettingsTo'),
      settings: t('media.allowPhotos'),
      retry: t('chat.tryAgain2'),
    },
  });
  return (
    <NativeAttachmentKeyboard
      testID="attachment-keyboard-host"
      style={{ position: 'absolute', width: 0, height: 0 }}
      binding={binding}
      model={model}
      onAction={({ nativeEvent: event }) => {
        if (
          event.token !== request.id ||
          event.context !== request.field.context ||
          event.revision !== revision ||
          !admitted()
        )
          return;
        const item = state.items.find((value) => value.id === event.id);
        switch (event.action) {
          case 'keyboard':
            keyboard.closeKeyboardPanel();
            break;
          case 'document':
          case 'audio':
            // Telegram replaces the category row after a selection. Preserve it
            // until review rather than silently discarding selected Photos.
            if (!state.selected.length) keyboard.handoff(event.token, event.action);
            break;
          case 'select':
            if (item) {
              void Haptics.selectionAsync().catch(() => {});
              session.select(item);
            }
            break;
          case 'preview':
            if (item) {
              if (!state.selected.some((value) => value.id === item.id)) session.select(item);
              const index = session
                .getSnapshot()
                .selected.findIndex((value) => value.id === item.id);
              if (index >= 0) {
                onPreferredIndex(index);
                void session.approve();
              }
            }
            break;
          case 'review':
            onPreferredIndex(null);
            void session.approve();
            break;
          case 'camera':
            session.camera();
            break;
          case 'more':
            void session.more();
            break;
          case 'manage':
            void session.manage();
            break;
          case 'settings':
            void Linking.openSettings();
            break;
          case 'retry':
            void session.reload();
            break;
          case 'album':
            if (event.id === '' || state.albums.some((album) => album.id === event.id))
              session.album(event.id || null);
            break;
        }
      }}
      onClosed={({ nativeEvent: event }) => {
        if (event.token !== request.id || event.context !== request.field.context) return;
        const latest = session.getSnapshot();
        if (latest.token !== state.token || !keyboard.owns(event.token)) return;
        // Native session/background/focus retirement is terminal, never an
        // approval of a previously queued Camera/Files/Audio intent.
        if (event.reason !== 'closed') {
          keyboard.retire();
          session.cancel();
          return;
        }
        keyboard.closed(event.token);
        // Approve/camera already hid state.visible and still own their pending
        // result. User/system close must cancel, not invoke either handoff.
        if (latest.visible) session.cancel();
        else void session.dismissed(latest.token);
      }}
    />
  );
}
