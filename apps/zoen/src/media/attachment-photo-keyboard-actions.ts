import * as Haptics from 'expo-haptics';
import { Linking } from 'react-native';
import { conversationAttachmentCategory } from './attachment-categories';
import type { AttachmentKeyboardState } from './attachment-keyboard-session';
import type {
  AttachmentCaptionDraft,
  AttachmentPhotoKeyboardProps,
} from './attachment-photo-keyboard.types';
import { photoSendOptions, type PhotoSendMode } from './attachment-photo-options';
import type { PhotoPickerSubmission } from './attachment-photo-session';
import { recentAttachmentId, type RecentAttachmentState } from './attachment-recent-session';
import type { NativeKeyboardAction } from './keyboard-panel-native.ios';
import type { MediaDraftAttachmentOptions } from './media-draft-types';

type ActionContext = Pick<
  AttachmentPhotoKeyboardProps,
  | 'state'
  | 'session'
  | 'keyboard'
  | 'request'
  | 'onPreferredIndex'
  | 'onSubmission'
  | 'recent'
  | 'sendAsFileAllowed'
  | 'existingOptions'
  | 'routes'
> & {
  revision: string;
  admitted(): boolean;
  recentActive: boolean;
  recentSelection: RecentAttachmentState['selected'];
  caption: string;
  gallery: boolean;
  currentDraft: AttachmentCaptionDraft;
  setDraft(value: AttachmentCaptionDraft): void;
  canEdit: boolean;
  owner: Pick<AttachmentKeyboardState, 'hasAudio'>;
};

/** Capture one rendered model; live admission still belongs to the mounted picker. */
export function attachmentKeyboardAction(
  context: ActionContext,
  { nativeEvent: event }: { nativeEvent: NativeKeyboardAction },
) {
  const {
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
  } = context;
  if (
    event.token !== request.id ||
    event.context !== request.field.context ||
    event.revision !== revision ||
    !admitted()
  )
    return;
  const item = [...state.items, ...state.selected].find((value) => value.id === event.id);
  const prepareRecent = (ids: readonly string[], text: string) => {
    if (
      !recentActive ||
      !recent ||
      !onSubmission ||
      !ids.length ||
      text.length > 4096 ||
      text.includes('\0') ||
      ids.some((id) => !recent.session.find(id))
    )
      return;
    void session.prepareFiles(async (current) => {
      const files = await recent.session.materialize(ids, current);
      if (current()) {
        onPreferredIndex(null);
        onSubmission({
          caption: text,
          captions: ids.map((_, index) => (index === 0 ? text : '')),
          send: true,
        });
      }
      return files;
    });
  };
  switch (event.action) {
    case 'recentOpen':
      if (recentActive && recent && event.id) {
        if (recentSelection.length) recent.session.select(event.id);
        else prepareRecent([event.id], caption);
      }
      break;
    case 'recentSelect':
      if (recentActive && recent && event.id) recent.session.select(event.id);
      break;
    case 'recentPreview':
      if (recentActive && recent && event.id && recent.session.find(event.id)) {
        const id = event.id;
        session.previewFile((current) => recent.session.preview(id, current));
      }
      break;
    case 'recentSend':
      if (typeof event.text === 'string')
        prepareRecent(recentSelection.map(recentAttachmentId), event.text);
      break;
    case 'recentSearch':
      if (recentActive && recent) recent.session.search(event.id !== 'cancel');
      break;
    case 'recentQuery':
      if (recentActive && recent && typeof event.text === 'string')
        recent.session.query(event.text);
      break;
    case 'recentMore':
      if (recentActive && recent) void recent.session.load(true);
      break;
    case 'recentRetry':
      if (recentActive && recent) void recent.session.load(false);
      break;
    case 'recentGallery':
      if (!recentSelection.length) session.showCategory('photos');
      break;
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
    case 'option':
      if (onSubmission && !state.busy) {
        const isOption = event.action === 'option';
        const asFile = isOption && event.id === 'send-as-file';
        const ungrouped = isOption && event.id === 'send-without-grouping';
        if (
          isOption &&
          ((!asFile && !ungrouped) ||
            (asFile && !sendAsFileAllowed) ||
            (ungrouped &&
              (state.selected.length < 2 ||
                Boolean(
                  existingOptions &&
                  (existingOptions.presentation !== 'media' ||
                    existingOptions.grouping !== 'separate'),
                ))) ||
            state.category !== 'photos' ||
            state.screen !== 'grid' ||
            !Array.isArray(event.ids) ||
            event.ids.length !== state.selected.length ||
            !event.ids.every((id, index) => id === state.selected[index]?.id) ||
            typeof event.text !== 'string')
        )
          return;
        if (event.action === 'edit' && !canEdit) return;
        if (gallery && (!item || event.id !== state.previewId)) return;
        if (!gallery && !isOption && event.id !== undefined) return;
        if (!session.canApprove(gallery ? item?.id : undefined)) return;
        const selected = [...state.selected];
        if (gallery && item && !selected.some((value) => value.id === item.id)) selected.push(item);
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
        const mode: PhotoSendMode = asFile
          ? 'send-as-file'
          : ungrouped
            ? 'send-without-grouping'
            : 'default';
        let options: MediaDraftAttachmentOptions | undefined;
        try {
          options = photoSendOptions(selected.length, mode, existingOptions);
        } catch {
          return;
        }
        onPreferredIndex(requestEdit ? selected.findIndex((value) => value.id === item?.id) : null);
        const intent: PhotoPickerSubmission = {
          caption: captions[0] ?? '',
          captions,
          send: !requestEdit,
          ...(requestEdit ? { requestEdit: true } : {}),
          ...(options ? { options } : {}),
        };
        // A failed/canceled export never leaves a terminal file-mode
        // intent behind for a later ordinary selection or Browse action.
        void session.approve(gallery ? item?.id : undefined, () => onSubmission(intent));
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
        !recentSelection.length &&
        state.category === event.action &&
        (event.action !== 'audio' || owner.hasAudio || recent)
      ) {
        onSubmission?.({ caption, captions: [], send: false });
        session.browse();
      }
      break;
    case 'category':
      if (recentSelection.length || state.selected.length) break;
      {
        const category = conversationAttachmentCategory(event.id);
        if (category && routes?.categories.includes(category)) {
          session.openRoute((current) => routes.open(category, current));
          break;
        }
      }
      if (
        event.id === 'photos' ||
        event.id === 'document' ||
        (event.id === 'audio' && (owner.hasAudio || recent))
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
}
