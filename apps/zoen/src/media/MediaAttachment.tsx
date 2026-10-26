import type { TimelineEntry } from '../core/types';
import FileAttachmentContent from './file-attachment-content';
import type { FileAttachmentProps, MediaTile, Presentation } from './file-attachment-types';
import { type DecodedMessageImage } from './inline-image-frame';
export type { MediaTile } from './file-attachment-types';

/** Generic media owns row copies; received voice controls use the account transport. */
export function FileAttachment(props: FileAttachmentProps) {
  return (
    <FileAttachmentContent
      key={`${props.itemId}:${props.version ?? 'latest'}:${props.expectedHash ?? ''}`}
      {...props}
    />
  );
}

export function MessageMedia({
  spaceId,
  entry,
  presentation = 'message',
  onActions,
  onImageDecoded,
  tile,
  onInlineUnavailable,
  galleryEntries,
  isCurrent,
}: {
  spaceId: string;
  entry: TimelineEntry;
  presentation?: Presentation;
  onActions?: () => void;
  onImageDecoded?: (image: DecodedMessageImage) => void;
  tile?: MediaTile;
  onInlineUnavailable?: () => void;
  galleryEntries?: readonly TimelineEntry[];
  isCurrent?: () => boolean;
}) {
  if (entry.deleted) return null;
  if (entry.voice)
    return (
      <FileAttachment
        key={`${entry.voice.item_id}:${entry.voice.sha256}`}
        itemId={entry.voice.item_id}
        voice={entry.voice}
        expectedHash={entry.voice.sha256}
        isCurrent={isCurrent}
        messageSource={{ spaceId, entry }}
        presentation={presentation}
        mine={entry.author.is_me}
        onActions={onActions}
      />
    );
  const card = 'Message' in entry.kind ? entry.kind.Message.card : null;
  return card?.kind_id === 'file' ? (
    <FileAttachment
      itemId={card.item_id}
      version={card.version}
      title={card.title}
      onImageDecoded={onImageDecoded}
      tile={tile}
      onInlineUnavailable={onInlineUnavailable}
      isCurrent={isCurrent}
      messageSource={{ spaceId, entry }}
      galleryEntries={galleryEntries}
      presentation={presentation}
      renderAsFile={entry.media?.presentation === 'file'}
      mine={entry.author.is_me}
      onActions={onActions}
    />
  ) : null;
}
