import type { ReactNode } from 'react';
import type { AttachmentComposerField } from './attachment-composer';
import type { AttachmentDraftState } from './attachment-draft';
import type { AttachmentPickKind } from './attachment-picker';

/** Display-only projection: the existing channel owns content, targets and Send. */
export type AttachmentApprovalProps = {
  testID?: string;
  /** Native sheet host follows the existing approval Modal presenter. */
  libraryPicker?: ReactNode;
  state: AttachmentDraftState;
  field: AttachmentComposerField;
  selectedId: string | null;
  recipient: string;
  busy: boolean;
  editing: boolean;
  canAdd: boolean;
  canEdit: boolean;
  error: string | null;
  isCurrent(): boolean;
  onClose(): void;
  onSelect(id: string): void;
  onEdit(): void;
  onAdd(kind: AttachmentPickKind): void;
  onRemove(id: string): void;
  onRetry(): void;
};
