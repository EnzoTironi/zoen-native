import { type ConversationAttachmentCategory } from './attachment-categories';
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
  type AttachmentRecentSession,
  type RecentAttachmentState,
} from './attachment-recent-session';
import type { MediaDraftAttachmentOptions } from './media-draft-types';
export type AttachmentPhotoKeyboardProps = {
  state: PhotoPickerState;
  session: AttachmentPhotoSession;
  keyboard: AttachmentKeyboardSession | null;
  request: KeyboardPresentation;
  onPreferredIndex(index: number | null): void;
  onSubmission?(submission: PhotoPickerSubmission): void;
  onResume?(presentation: KeyboardPresentation): void;
  recent?: { session: AttachmentRecentSession; state: RecentAttachmentState };
  sendAsFileAllowed?: boolean;
  existingOptions?: MediaDraftAttachmentOptions;
  routes?: {
    categories: readonly ConversationAttachmentCategory[];
    open(category: ConversationAttachmentCategory, current: () => boolean): void;
  };
};
export type AttachmentCaptionDraft = {
  token: number;
  forced: string | null;
  items: Readonly<Record<string, string>>;
};
