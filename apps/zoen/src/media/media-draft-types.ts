import type { TimelineEntry } from '../core/types';
import type { MediaReplyFields } from './media-reply';

/** Device-local Rust journal. Source bytes never cross into JS persistence. */
export type MediaDraftScope = Readonly<{
  expected_identity_id: string;
  expected_device_id: string;
  space_id: string;
  thread_root_id: string | null;
}>;
export type MediaDraftAsset = Readonly<{
  id: string;
  name: string;
  mime: string;
  size: number;
  sha256: string;
}>;
export type MediaDraftIngest = Readonly<{
  id: string;
  uri: string;
  name: string;
  mime: string;
  voice_source_id?: string;
  voice_role?: 'source' | 'render';
}>;
export type MediaDraftAttachment = Readonly<{
  asset_id: string;
  caption: string;
  mutation_id: string;
}>;
export type MediaDraftAttachmentOptions = Readonly<{
  presentation: 'media' | 'file';
  grouping: 'separate' | 'grouped';
}>;
export type MediaDraftGallery = Readonly<{
  entries: readonly Readonly<{ asset_id: string; photo_id: string }>[];
  forced_caption: boolean;
}>;
export type MediaDraftAttachments = MediaReplyFields &
  Readonly<{
    kind: 'attachments';
    reply_to: string | null;
    items: readonly MediaDraftAttachment[];
    /** Private picker recovery; the referenced bytes belong to the same Rust journal. */
    gallery?: MediaDraftGallery;
    /** Omitted for the legacy media/separate defaults; persisted by the Rust journal. */
    options?: MediaDraftAttachmentOptions;
  }>;
export type MediaDraftVoice = MediaReplyFields &
  Readonly<{
    kind: 'voice';
    reply_to: string | null;
    source_asset_id: string;
    /** Validated by the voice editor when restored; contains no file URI or native lease. */
    editor: unknown;
    render: Readonly<{
      asset_id: string;
      mime: string;
      duration_ms: number;
      levels: readonly number[];
      transcript: string;
    }> | null;
  }>;
export type MediaDraftPollOption = Readonly<{ id: string; text: string }>;
export type MediaDraftPoll = MediaReplyFields &
  Readonly<{
    kind: 'poll';
    reply_to: string | null;
    question: string;
    options: readonly MediaDraftPollOption[];
  }>;
export type MediaDraftContent = MediaDraftAttachments | MediaDraftVoice | MediaDraftPoll;
export type MediaDraftDto = Readonly<{
  revision: string;
  content_json: string;
  assets: readonly MediaDraftAsset[];
  attempted: boolean;
  accepted_intent_json?: string | null;
}>;
export type MediaDraftSendResult = Readonly<{
  entries: readonly TimelineEntry[];
  draft: MediaDraftDto;
}>;
export type MediaDraftSaveResult = Readonly<{
  /** The revision actually accepted by this save, even when current content is newer. */
  accepted_revision: string;
  draft: MediaDraftDto;
}>;
export type MaterializedMediaDraftAsset = MediaDraftAsset & Readonly<{ uri: string }>;
export interface MediaDraftPort {
  load(scope: MediaDraftScope): Promise<MediaDraftDto>;
  /** Copies a refused send to a new private draft. Never sends or retries it. */
  recoverFailed?(
    scope: MediaDraftScope,
    messageId: string,
    expectedRevision: string,
    expectedMessageRevision: string,
  ): Promise<MediaDraftDto>;
  save(
    scope: MediaDraftScope,
    expectedRevision: string,
    contentJson: string,
    assets: readonly MediaDraftIngest[],
  ): Promise<MediaDraftSaveResult>;
  materialize(
    scope: MediaDraftScope,
    expectedRevision: string,
    assetId: string,
  ): Promise<MaterializedMediaDraftAsset>;
  /** Native read-only copy; resolves only after actual viewer dismissal and cleanup. */
  preview?(
    scope: MediaDraftScope,
    expectedRevision: string,
    assetId: string,
    presentationId: string,
  ): Promise<void>;
  cancelPreview?(presentationId: string): Promise<void>;
  /** Native edited-copy review; returns the canonical journal after dismissal, never sends. */
  edit?(
    scope: MediaDraftScope,
    expectedRevision: string,
    assetId: string,
    presentationId: string,
  ): Promise<MediaDraftDto>;
  send(scope: MediaDraftScope, expectedRevision: string): Promise<MediaDraftSendResult>;
  release(uri: string): Promise<void>;
}
