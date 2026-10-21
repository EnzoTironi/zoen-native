import { useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { randomUUID } from 'expo-crypto';
import { Linking, useWindowDimensions } from 'react-native';
import { isLiquidGlassAvailable } from 'expo-glass-effect';
import * as Haptics from 'expo-haptics';
import { t } from '../i18n';
import type {
  AttachmentKeyboardSession,
  KeyboardPresentation,
} from './attachment-keyboard-session';
import type {
  AttachmentPhotoSession,
  PhotoPickerState,
  PhotoPickerSubmission,
} from './attachment-photo-session';
import {
  NativeAttachmentKeyboard,
  NativeStandaloneAttachmentPanel,
  useKeyboardPanelBinding,
} from './keyboard-panel-native.ios';
const standaloneOwner = { enabled: true, field: null, presentation: null, hasAudio: false };
const standaloneSnapshot = () => standaloneOwner;
const standaloneSubscribe = () => () => {};

export function AttachmentPhotoKeyboard({
  state,
  session,
  keyboard,
  request,
  onPreferredIndex,
  onSubmission,
  onResume,
}: {
  state: PhotoPickerState;
  session: AttachmentPhotoSession;
  keyboard: AttachmentKeyboardSession | null;
  request: KeyboardPresentation;
  onPreferredIndex(index: number | null): void;
  onSubmission?(submission: PhotoPickerSubmission): void;
  onResume?(presentation: KeyboardPresentation): void;
}) {
  const [draft, setDraft] = useState<{
    token: number;
    forced: string | null;
    items: Readonly<Record<string, string>>;
  }>({ token: state.token, forced: null, items: {} });
  const currentDraft =
    draft.token === state.token ? draft : { token: state.token, forced: null, items: {} };
  if (draft.token !== state.token) setDraft(currentDraft);
  const gallery = state.screen === 'preview' || state.screen === 'selected';
  const previewItem = [...state.items, ...state.selected].find(
    (item) => item.id === state.previewId,
  );
  const sendCount =
    state.selected.length +
    (state.screen === 'preview' &&
    previewItem &&
    !state.selected.some((item) => item.id === previewItem.id)
      ? 1
      : 0);
  const canEdit = Boolean(
    onSubmission &&
    gallery &&
    previewItem?.mediaType === 'image' &&
    /\.(jpe?g|png|heic|heif)$/i.test(previewItem.filename ?? ''),
  );
  const caption =
    currentDraft.forced ??
    (gallery && state.previewId ? (currentDraft.items[state.previewId] ?? '') : '');
  const owner = useSyncExternalStore(
    keyboard?.subscribe ?? standaloneSubscribe,
    keyboard?.getSnapshot ?? standaloneSnapshot,
    keyboard?.getSnapshot ?? standaloneSnapshot,
  );
  const { width, height } = useWindowDimensions();
  const presentation = owner.presentation?.id === request.id ? owner.presentation : request;
  const renderOwner = useMemo(
    () => ({ state, request, owner, revision: randomUUID() }),
    [state, request, owner],
  );
  const revision = renderOwner.revision;
  const committed = useRef<object | null>(null);
  const liveRequest = useRef<string | null>(request.id);
  const closedRequest = useRef<string | null>(null);
  useLayoutEffect(() => {
    liveRequest.current = request.id;
    return () => {
      if (liveRequest.current === request.id) liveRequest.current = null;
    };
  }, [request.id]);
  useLayoutEffect(() => {
    committed.current = renderOwner;
    return () => {
      if (committed.current === renderOwner) committed.current = null;
    };
  }, [renderOwner]);
  const admitted = () =>
    committed.current === renderOwner &&
    liveRequest.current === request.id &&
    closedRequest.current !== request.id &&
    (!keyboard || keyboard.isCurrent(request.id)) &&
    session.isInteractive(state.token);
  const binding = useKeyboardPanelBinding(
    presentation,
    owner.enabled &&
      (!keyboard || keyboard.owns(request.id)) &&
      !presentation.closing &&
      state.visible &&
      (state.permission !== null || state.error !== null),
    presentation.closing || !state.visible,
  );
  const model = JSON.stringify({
    revision,
    columns: width > height ? 5 : 3,
    screen: state.screen,
    activeCategory: state.category,
    canEdit,
    fileItems: [],
    audioItems: [],
    previewId: state.previewId,
    items: state.items.map((item) => ({
      id: item.id,
      type: item.mediaType,
      name: item.filename,
      duration: item.duration,
      width: item.width,
      height: item.height,
    })),
    selectedItems: state.selected.map((item) => ({
      id: item.id,
      type: item.mediaType,
      name: item.filename,
      duration: item.duration,
      width: item.width,
      height: item.height,
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
    cameraAvailable: true,
    glass: isLiquidGlassAvailable(),
    ...(onSubmission
      ? {
          caption: {
            value: caption,
            placeholder: t('media.attachmentCaptionPlaceholder'),
            editable: !state.busy,
            sending: state.busy,
            count: state.selected.length,
            sendEnabled: session.canApprove(gallery ? (state.previewId ?? '') : undefined),
            sendLabel: t('media.attachmentSendCount', { count: sendCount }),
          },
        }
      : {}),
    labels: {
      photos: t('media.attachmentGallery'),
      files: t('media.file2'),
      audio: t('media.attachmentAudio'),
      camera: t('media.camera'),
      recents: t('media.photoLibraryRecents'),
      review: t('media.photoLibraryReview', { count: state.selected.length }),
      preparing: t('media.attachmentPreparing'),
      keyboard: t('composerKeyboard.keyboard'),
      close: t('media.cancelAttachment'),
      back: t('media.attachmentBack'),
      edit: t('media.attachmentEdit'),
      remove: t('media.attachmentRemove', { name: previewItem?.filename ?? '' }),
      more: t('media.attachmentMore'),
      add: t('media.attachmentAdd'),
      select: t('media.attachmentSelectMedia'),
      selected: t('media.attachmentSelectedCount', { count: state.selected.length }),
      messagePreview: t('media.attachmentMessagePreview'),
      dragToReorder: t('media.attachmentDragToReorder'),
      fileTitle: t('media.file2'),
      audioTitle: t('media.attachmentAudio'),
      browseFiles: t('media.attachmentBrowseFiles'),
      filesIntro: t('media.attachmentFilesIntro'),
      manage: t('media.photoLibraryManageAccess'),
      permission: t('media.youCanAllowAccessInDeviceSettingsTo'),
      settings: t('media.allowPhotos'),
      retry: t('chat.tryAgain2'),
    },
  });
  const NativePanel = keyboard ? NativeAttachmentKeyboard : NativeStandaloneAttachmentPanel;
  return (
    <NativePanel
      testID="attachment-keyboard-host"
      style={{ position: 'absolute', width: 0, height: 0 }}
      binding={
        keyboard
          ? binding
          : JSON.stringify({
              ...JSON.parse(binding),
              tag: undefined,
              label: undefined,
              placeholder: undefined,
              standalone: true,
            })
      }
      model={model}
      onAction={({ nativeEvent: event }) => {
        if (
          event.token !== request.id ||
          event.context !== request.field.context ||
          event.revision !== revision ||
          !admitted()
        )
          return;
        const item = [...state.items, ...state.selected].find((value) => value.id === event.id);
        switch (event.action) {
          case 'caption':
            if (
              onSubmission &&
              typeof event.text === 'string' &&
              event.text.length <= 4096 &&
              !event.text.includes('\0')
            ) {
              if (gallery ? !item || event.id !== state.previewId : event.id !== undefined) return;
              if (gallery && item && currentDraft.forced === null) {
                setDraft({
                  ...currentDraft,
                  items: { ...currentDraft.items, [item.id]: event.text },
                });
                // LegacyMediaPickerGallery stores a caption on its current
                // asset and selects that asset when the caption is nonempty.
                if (event.text && !state.selected.some((value) => value.id === item.id))
                  session.select(item);
              } else setDraft({ ...currentDraft, forced: event.text });
            }
            break;
          case 'send':
          case 'edit':
            if (onSubmission && !state.busy) {
              if (event.action === 'edit' && !canEdit) return;
              if (gallery && (!item || event.id !== state.previewId)) return;
              if (!gallery && event.id !== undefined) return;
              if (!session.canApprove(gallery ? item?.id : undefined)) return;
              const selected = [...state.selected];
              if (gallery && item && !selected.some((value) => value.id === item.id))
                selected.push(item);
              if (!selected.length) return;
              const finalCaption = event.text ?? caption;
              if (
                typeof finalCaption !== 'string' ||
                finalCaption.length > 4096 ||
                finalCaption.includes('\0')
              )
                return;
              const forced = gallery ? currentDraft.forced : finalCaption;
              const captions = selected.map((value, index) =>
                forced !== null
                  ? index === 0
                    ? finalCaption
                    : ''
                  : value.id === item?.id
                    ? finalCaption
                    : (currentDraft.items[value.id] ?? ''),
              );
              const requestEdit = event.action === 'edit';
              onPreferredIndex(
                requestEdit ? selected.findIndex((value) => value.id === item?.id) : null,
              );
              onSubmission({
                caption: captions[0] ?? '',
                captions,
                send: !requestEdit,
                ...(requestEdit ? { requestEdit: true } : {}),
              });
              void session.approve(gallery ? item?.id : undefined);
            }
            break;
          case 'keyboard':
            if (keyboard) keyboard.closeKeyboardPanel();
            else session.cancel();
            break;
          case 'document':
          case 'audio':
            // Telegram replaces the category row after a selection. Preserve it
            // until review rather than silently discarding selected Photos.
            if (
              !state.selected.length &&
              state.category === event.action &&
              (event.action !== 'audio' || owner.hasAudio)
            ) {
              if (event.action === 'document') session.browse();
              else if (keyboard) keyboard.handoff(event.token, event.action);
            }
            break;
          case 'category':
            if (
              event.id === 'photos' ||
              event.id === 'document' ||
              (event.id === 'audio' && owner.hasAudio)
            )
              session.showCategory(event.id);
            break;
          case 'select':
            if (item) {
              void Haptics.selectionAsync().catch(() => {});
              session.select(item);
            }
            break;
          case 'preview':
            if (item) session.preview(item.id);
            break;
          case 'back':
            session.back();
            break;
          case 'albums':
            session.showAlbums();
            break;
          case 'albumDetails':
            if (event.id) void session.albumDetails(event.id);
            break;
          case 'selected':
            session.showSelected();
            break;
          case 'reorder':
            if (
              Array.isArray(event.ids) &&
              event.ids.length <= 100 &&
              event.ids.every((id) => typeof id === 'string' && id.length <= 2048)
            )
              session.reorder(event.ids);
            break;
          case 'camera':
            onSubmission?.({
              caption:
                currentDraft.forced ??
                (state.selected[0] ? (currentDraft.items[state.selected[0].id] ?? '') : ''),
              captions: state.selected.map((item, index) =>
                currentDraft.forced !== null
                  ? index === 0
                    ? currentDraft.forced
                    : ''
                  : (currentDraft.items[item.id] ?? ''),
              ),
              send: false,
            });
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
        if (
          event.token !== request.id ||
          event.token !== liveRequest.current ||
          event.token === closedRequest.current ||
          event.context !== request.field.context
        )
          return;
        const latest = session.getSnapshot();
        if (latest.token !== state.token || (keyboard && !keyboard.owns(event.token))) return;
        closedRequest.current = event.token;
        // Native session/background/focus retirement is terminal, never an
        // approval of a previously queued Camera/Files/Audio intent.
        if (event.reason !== 'closed') {
          keyboard?.retire();
          session.cancel();
          return;
        }
        keyboard?.closed(event.token);
        // Approve/camera already hid state.visible and still own their pending
        // result. User/system close must cancel, not invoke either handoff.
        if (latest.visible) session.cancel();
        else
          void session.dismissed(latest.token).then(() => {
            const resumed = session.getSnapshot();
            if (resumed.token !== latest.token || !resumed.visible || !onResume) return;
            const next = keyboard
              ? keyboard.resumePicker(event.token)
              : { ...request, id: randomUUID(), closing: false, returnToKeyboard: false };
            if (next) onResume(next);
          });
      }}
    />
  );
}
