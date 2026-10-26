import { requireNativeView } from 'expo';
import { randomUUID } from 'expo-crypto';
import { useLayoutEffect, useMemo, useRef } from 'react';
import { type ViewProps } from 'react-native';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { AttachmentApprovalProps } from './attachment-approval.types';

export const supportsMediaApproval = true;
type GalleryAction = {
  token: string;
  context: string;
  revision: string;
  action: string;
  id?: string;
  text?: string;
  kind?: string;
  ids?: string[];
};
type GalleryClosed = { token: string; context: string; reason: string };
type NativeProps = ViewProps & {
  binding: string;
  model: string;
  onAction(event: { nativeEvent: GalleryAction }): void;
  onClosed(event: { nativeEvent: GalleryClosed }): void;
};
const NativeGallery = requireNativeView<NativeProps>('ZoenCore', 'ZoenAttachmentGalleryHost');

/** The Photos picker and durable review share the same native Telegram gallery.
 * This projection never copies files or acquires an alternate draft authority. */
export function AttachmentApproval(props: AttachmentApprovalProps) {
  const { state, field, selectedId, recipient, busy, editing, canAdd, canEdit, error } = props;
  const { presentation: theme } = useTheme();
  const selected = state.items.find((item) => item.mutationId === selectedId) ?? state.items[0];
  const context = JSON.stringify(field.scope);
  const session = useMemo(() => ({ context, token: randomUUID() }), [context]);
  const token = session.token;
  const owner = useMemo(() => ({ props, token, revision: randomUUID() }), [props, token]);
  const committed = useRef<typeof owner | null>(null);
  useLayoutEffect(() => {
    committed.current = owner;
    return () => {
      if (committed.current === owner) committed.current = null;
    };
  }, [owner]);
  if (!selected) return null;
  const admitted = () =>
    committed.current === owner && props.isCurrent() && !busy && !editing && !props.suspended;
  const binding = JSON.stringify({
    token,
    context,
    active: true,
    suspended: Boolean(props.suspended || editing),
    appearance: {
      background: theme.actionSheet.background,
      text: theme.text,
      secondary: theme.secondary,
      accent: theme.accent,
      separator: theme.separator,
    },
  });
  const model = JSON.stringify({
    revision: owner.revision,
    source: 'draft',
    grouped: props.grouped === true,
    screen: 'preview',
    previewId: selected.mutationId,
    items: state.items.map((item) => ({
      id: item.mutationId,
      uri: item.uri,
      name: item.name,
      mime: item.mime,
      sha256: item.sha256,
      locationErrorLabel: t('attachmentTabs.locationOpenFailed'),
    })),
    selected: state.items.map((item) => item.mutationId),
    recipient,
    busy: busy || editing,
    canAdd,
    canEdit,
    canReorder: Boolean(props.onReorder),
    canRemove: !selected.attempted && !busy && !editing,
    errorText: error ?? (field.retry ? t('media.attachmentRetryLocked') : null),
    caption: {
      value: field.value,
      placeholder: t('media.attachmentCaptionPlaceholder'),
      editable: field.editable,
      sending: field.sending,
      sendEnabled: !field.sending && field.remaining > 0,
      sendLabel: field.retry
        ? t('media.attachmentsRetry')
        : t('media.attachmentSendCount', { count: field.remaining }),
    },
    labels: {
      close: t('common.close'),
      back: t('common.close'),
      edit: t('media.attachmentEdit'),
      add: t('media.addAttachment'),
      remove: t('media.attachmentRemove', { name: selected.name }),
      review: t('media.attachmentDraftCount', { count: state.items.length }),
      retry: t('chat.tryAgain2'),
      photos: t('media.photosAndVideos'),
      camera: t('media.camera'),
      files: t('media.file2'),
      select: t('media.attachmentSelectMedia'),
      messagePreview: t('media.attachmentMessagePreview'),
      dragToReorder: t('media.attachmentDragToReorder'),
    },
  });
  return (
    <>
      {props.libraryPicker}
      <NativeGallery
        testID={props.testID ?? 'attachment-approval'}
        style={{ position: 'absolute', width: 0, height: 0 }}
        binding={binding}
        model={model}
        onAction={({ nativeEvent: event }) => {
          if (
            event.token !== token ||
            event.context !== context ||
            event.revision !== owner.revision ||
            !admitted()
          )
            return;
          const item = state.items.find((value) => value.mutationId === event.id);
          switch (event.action) {
            case 'preview':
              if (item) props.onSelect(item.mutationId);
              break;
            case 'caption':
              if (
                item === selected &&
                field.editable &&
                typeof event.text === 'string' &&
                event.text.length <= 4096 &&
                !event.text.includes('\0')
              )
                field.onChange(event.text);
              break;
            case 'send':
              if (
                item === selected &&
                !field.sending &&
                field.remaining > 0 &&
                typeof event.text === 'string' &&
                event.text.length <= 4096 &&
                !event.text.includes('\0')
              )
                field.onSend(event.text);
              break;
            case 'edit':
              if (
                item === selected &&
                canEdit &&
                typeof event.text === 'string' &&
                event.text.length <= 4096 &&
                !event.text.includes('\0')
              )
                props.onEdit(event.text);
              break;
            case 'remove':
              if (item === selected && !item.attempted) props.onRemove(item.mutationId);
              break;
            case 'add':
              if (
                canAdd &&
                (event.kind === 'photo' || event.kind === 'camera' || event.kind === 'document')
              )
                props.onAdd(event.kind);
              break;
            case 'retry':
              props.onRetry();
              break;
            case 'reorder':
              if (
                props.onReorder &&
                Array.isArray(event.ids) &&
                event.ids.length === state.items.length &&
                new Set(event.ids).size === event.ids.length &&
                event.ids.every((id) => state.items.some((value) => value.mutationId === id))
              )
                props.onReorder(event.ids);
              break;
            case 'close':
              props.onClose();
              break;
            case 'back':
              props.onClose();
              break;
          }
        }}
        onClosed={({ nativeEvent: event }) => {
          if (event.token === token && event.context === context && committed.current === owner)
            props.onClose();
        }}
      />
    </>
  );
}
