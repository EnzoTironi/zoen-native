import { randomUUID } from 'expo-crypto';
import { isLiquidGlassAvailable } from 'expo-glass-effect';
import { useLayoutEffect, useMemo, useRef, useSyncExternalStore } from 'react';
import { useWindowDimensions } from 'react-native';
import { t } from '../i18n';
import { attachmentKeyboardAction } from './attachment-photo-keyboard-actions';
import type {
  AttachmentCaptionDraft,
  AttachmentPhotoKeyboardProps,
} from './attachment-photo-keyboard.types';
import { recentAttachmentId } from './attachment-recent-session';
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
  recent,
  sendAsFileAllowed = false,
  existingOptions,
  routes,
}: AttachmentPhotoKeyboardProps) {
  const currentDraft: AttachmentCaptionDraft = { token: state.token, ...state.captions };
  const setDraft = (next: AttachmentCaptionDraft) => {
    if (next.token === state.token) session.setCaptions(next);
  };
  const gallery = state.screen === 'preview' || state.screen === 'selected';
  const recentActive = state.category !== 'photos' && recent !== undefined;
  const recentSelection = recentActive ? recent.state.selected : [];
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
    ((previewItem?.mediaType === 'image' &&
      /\.(jpe?g|png|heic|heif)$/i.test(previewItem.filename ?? '')) ||
      (previewItem?.mediaType === 'video' && /\.(mp4|mov|m4v)$/i.test(previewItem.filename ?? ''))),
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
    () => ({
      // Caption text does not replace the observed native input owner.
      state: {
        token: state.token,
        visible: state.visible,
        busy: state.busy,
        permission: state.permission,
        items: state.items,
        selected: state.selected,
        albums: state.albums,
        albumId: state.albumId,
        loading: state.loading,
        hasMore: state.hasMore,
        error: state.error,
        screen: state.screen,
        previewId: state.previewId,
        category: state.category,
      },
      request,
      owner,
      recent: recent?.state,
      sendAsFileAllowed,
      existingOptions,
      routes,
      revision: randomUUID(),
    }),
    [
      state.token,
      state.visible,
      state.busy,
      state.permission,
      state.items,
      state.selected,
      state.albums,
      state.albumId,
      state.loading,
      state.hasMore,
      state.error,
      state.screen,
      state.previewId,
      state.category,
      request,
      owner,
      recent?.state,
      sendAsFileAllowed,
      existingOptions,
      routes,
    ],
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
    session.isInteractive(state.token) &&
    session.getSnapshot().screen === state.screen &&
    session.getSnapshot().previewId === state.previewId &&
    session.getSnapshot().selected === state.selected &&
    session.getSnapshot().category === state.category;
  const binding = useKeyboardPanelBinding(
    presentation,
    owner.enabled &&
      (!keyboard || keyboard.owns(request.id)) &&
      !presentation.closing &&
      state.visible &&
      (state.permission !== null || state.error !== null),
    presentation.closing || !state.visible,
  );
  const displayItem = (photo: (typeof state.items)[number]) => {
    const retained = Object.hasOwn(state.previewUris, photo.id);
    const item = retained ? (state.selected.find((item) => item.id === photo.id) ?? photo) : photo;
    return {
      id: item.id,
      type: item.mediaType,
      name: item.filename,
      duration: item.duration,
      width: item.width,
      height: item.height,
      ...(retained ? { retained: true, uri: state.previewUris[item.id] } : {}),
    };
  };
  const model = JSON.stringify({
    revision,
    columns: width > height ? 5 : 3,
    screen: state.screen,
    activeCategory: state.category,
    ...(routes
      ? {
          categories: [
            { id: 'photos', label: t('media.attachmentGallery') },
            { id: 'document', label: t('media.file2') },
            ...(owner.hasAudio || recent
              ? [{ id: 'audio', label: t('media.attachmentAudio') }]
              : []),
            ...routes.categories.map((id) => ({ id, label: t(`attachmentTabs.${id}`) })),
          ],
        }
      : {}),
    canEdit,
    fileItems:
      recent?.state.kind === 'file'
        ? recent.state.items.map((row) => ({ ...row, id: recentAttachmentId(row) }))
        : [],
    audioItems:
      recent?.state.kind === 'audio'
        ? recent.state.items.map((row) => ({ ...row, id: recentAttachmentId(row) }))
        : [],
    recentSupported: Boolean(recent),
    recent: recentActive
      ? {
          selected: recentSelection.map(recentAttachmentId),
          query: recent.state.query,
          searching: recent.state.searching,
          loading: recent.state.loading,
          hasMore: recent.state.nextCursor !== null,
          error: Boolean(recent.state.error),
        }
      : null,
    previewId: state.previewId,
    items: state.items.map(displayItem),
    selectedItems: state.selected.map(displayItem),
    selected: state.selected.map((item) => item.id),
    grouped: existingOptions ? existingOptions.grouping === 'grouped' : true,
    sendOptions:
      onSubmission &&
      state.category === 'photos' &&
      state.screen === 'grid' &&
      state.selected.length > 0
        ? [
            ...(sendAsFileAllowed
              ? [
                  {
                    id: 'send-as-file',
                    title: t('media.attachmentSendAsFile', { count: state.selected.length }),
                    enabled: session.canApprove(),
                  },
                ]
              : []),
            ...(state.selected.length > 1 &&
            (!existingOptions ||
              (existingOptions.presentation === 'media' && existingOptions.grouping === 'separate'))
              ? [
                  {
                    id: 'send-without-grouping',
                    title: t('media.attachmentSendWithoutGrouping'),
                    enabled: session.canApprove(),
                  },
                ]
              : []),
          ]
        : [],
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
    hasAudio: Boolean(recent || owner.hasAudio),
    cameraAvailable: true,
    glass: isLiquidGlassAvailable(),
    ...(onSubmission
      ? {
          caption: {
            value: caption,
            placeholder: t('media.attachmentCaptionPlaceholder'),
            editable: !state.busy,
            sending: state.busy,
            count: recentActive ? recentSelection.length : state.selected.length,
            sendEnabled: recentActive
              ? recentSelection.length > 0
              : session.canApprove(gallery ? (state.previewId ?? '') : undefined),
            sendLabel: t('media.attachmentSendCount', {
              count: recentActive ? recentSelection.length : sendCount,
            }),
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
      recentlySentFiles: t('media.attachmentRecentlySentFiles'),
      chatsMusic: t('media.attachmentChatsMusic'),
      search: t('chat.search'),
      cancelSearch: t('common.cancel'),
      noResults: t('media.attachmentNoRecentResults'),
      loadingRecent: t('media.attachmentLoadingRecent'),
      recentError: t('media.attachmentRecentError'),
      loadMore: t('media.attachmentLoadMore'),
      previewFile: t('media.attachmentPreviewFile'),
      selectFile: t('media.attachmentSelectFile'),
      chooseGallery: t('media.attachmentGallery'),
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
      onAction={(event) =>
        attachmentKeyboardAction(
          {
            state,
            session,
            keyboard,
            request,
            onPreferredIndex,
            onSubmission,
            recent,
            sendAsFileAllowed,
            existingOptions,
            routes,
            revision,
            admitted,
            recentActive,
            recentSelection,
            caption,
            gallery,
            currentDraft,
            setDraft,
            canEdit,
            owner,
          },
          event,
        )
      }
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
          session.retire();
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
