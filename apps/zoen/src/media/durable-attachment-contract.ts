import type { Dispatch, RefObject, SetStateAction } from 'react';
import type { ClientStore } from '../core/store';
import type { Snapshot } from '../core/types';
import type { SemanticColors } from '../theme';
import type { AttachmentComposerOptions } from './attachment-composer';
import type { PhotoPickerAdmission, PhotoPickerSubmission } from './attachment-photo-types';
import { type AttachmentPickKind } from './attachment-picker';
import type { AttachmentReview } from './attachment-review';
import type { ApprovalPresentation, Preview } from './durable-attachment-types';
import type { MediaDraftChannel } from './media-draft-channel';
import type { MediaDraftAttachments, MediaDraftContent, MediaDraftDto } from './media-draft-types';

/** Borrowed previews and render values; the composer retains their lifetime. */
export type DurableAttachmentRuntime = {
  publish: (
    patch: Partial<{
      channel: MediaDraftChannel | null;
      picking: boolean;
      clearing: boolean;
      selected: string | null;
      approval: ApprovalPresentation | null;
      previews: ReadonlyMap<string, Preview>;
      error: string | null;
    }>,
  ) => void;
  channel: MediaDraftChannel | null;
  current: () => boolean;
  runtime: {
    channel: MediaDraftChannel | null;
    closed: boolean;
    focused: boolean;
    foreground: boolean;
    picking: boolean;
    submitting: boolean;
    approval: ApprovalPresentation | null;
    previews: Map<string, Preview>;
  };
  review: AttachmentReview;
  retireApproval: () => void;
  state: Readonly<{
    phase: 'loading' | 'ready' | 'closed';
    value: MediaDraftDto | null;
    content: MediaDraftContent | null;
    saving: boolean;
    sending: boolean;
    reviewing: boolean;
    frozen: boolean;
    error: Error | null;
  }> | null;
  photos: {
    pick: (
      remaining: number,
      current: () => boolean,
      _admission?: PhotoPickerAdmission,
    ) => Promise<Readonly<{ uri: string; name: string; mime: string; size: number }>[]>;
    cancel: () => void;
    view: null;
    preferredUri: () => string | null;
    submission: () => PhotoPickerSubmission | null;
  };
  store: ClientStore;
  admission: RefObject<AttachmentComposerOptions>;
  own: {
    channel: MediaDraftChannel | null;
    picking: boolean;
    clearing: boolean;
    selected: string | null;
    approval: ApprovalPresentation | null;
    previews: ReadonlyMap<string, Preview>;
    error: string | null;
  } | null;
  content: MediaDraftAttachments | null;
  reviewing: Readonly<{ mutationId: string | null; error: Error | null }>;
  options: AttachmentComposerOptions;
  canReview: boolean;
  colors: SemanticColors;
  snapshot: Snapshot | null;
  setRestoreEpoch: Dispatch<SetStateAction<number>>;
  setLocal: Dispatch<
    SetStateAction<{
      channel: MediaDraftChannel | null;
      picking: boolean;
      clearing: boolean;
      selected: string | null;
      approval: ApprovalPresentation | null;
      previews: ReadonlyMap<string, Preview>;
      error: string | null;
    }>
  >;
};
export type DurableAttachmentActions = {
  failure(reason: unknown): void;
  openApproval(id?: string): void;
  closeApproval(): Promise<void>;
  pick(kind: AttachmentPickKind): Promise<void>;
  cancel(): Promise<void>;
  remove(id: string): Promise<void>;
  reorder(ids: readonly string[]): Promise<void>;
  edit(
    mutationId: string,
    presentation: ApprovalPresentation,
    finalCaption?: string,
  ): Promise<void>;
  send(
    observed: MediaDraftAttachments,
    presentation: ApprovalPresentation | null,
    finalCaption?: { mutationId: string; text: string },
  ): Promise<void>;
};
