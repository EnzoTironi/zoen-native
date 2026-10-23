import type { TimelineEntry } from '../core/types';
import { type DecodedMessageImage } from './inline-image-frame';

export type Presentation = 'message' | 'document';
export type MediaTile = Readonly<{ width: number; height: number }>;
export type FileAttachmentProps = {
  itemId: string;
  /** Optional live document/route projection fence, independent of React unmount timing. */
  isCurrent?: () => boolean;
  version?: number;
  title?: string;
  voice?: TimelineEntry['voice'];
  expectedHash?: string;
  presentation?: Presentation;
  renderAsFile?: boolean;
  mine?: boolean;
  onActions?: () => void;
  onImageDecoded?: (image: DecodedMessageImage) => void;
  messageSource?: { spaceId: string; entry: TimelineEntry };
  galleryEntries?: readonly TimelineEntry[];
  tile?: MediaTile;
  onInlineUnavailable?: () => void;
};
