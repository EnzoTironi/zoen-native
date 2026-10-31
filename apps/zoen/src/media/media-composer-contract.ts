import type { AudioRecorder, RecorderState } from 'expo-audio';
import type { Dispatch, RefObject, SetStateAction } from 'react';
import { type ReactNode } from 'react';
import type { ClientStore } from '../core/store.ts';
import type { SemanticColors } from '../theme';
import { type AttachmentComposerField } from './attachment-composer';
import type { Recording, StoppedCapture } from './media-composer-shared';
import type { MediaDraftChannel } from './media-draft-channel.ts';
import type { MediaDraftContent, MediaDraftDto } from './media-draft-types.ts';
import type { MediaDriver } from './types.ts';
import type { MediaReplyFields, MediaReplyOptions } from './media-reply';
import type { AcceptedMediaReply } from './media-acceptance';
import type { VoiceDraftOwner } from './voice-draft-owner';
import type { VoiceDraft } from './voice-edit';
import type { VoiceReviewOwner } from './voice-review-owner';
import { type VoiceSession } from './voice-session';
import { type PendingVoiceTrim } from './voice-trim-preview';
export type CaptureLease = { owner: object; recover: () => Promise<void> };
export type VoiceCaptureContext = Readonly<{
  channel: MediaDraftChannel | null;
  stage: (capture: Omit<Recording, 'owner'>, preserveExpansion?: boolean, control?: object) => void;
  replyTo: string | null;
  replyContext: MediaReplyFields;
}>;
export type MediaComposerOptions = MediaReplyOptions & {
  spaceId: string;
  replyTo?: string | null;
  thread?: boolean;
  threadRootId?: string | null;
  disabled?: boolean;
  suspended?: boolean;
  onSent?: (reply?: AcceptedMediaReply) => void;
  onReplyAccepted?: (reply: AcceptedMediaReply) => void;
  onReplyCancelled?: (reply: AcceptedMediaReply) => void;
  renderControls?: (controls: {
    attachment: ReactNode;
    voice: ReactNode;
    review?: ReactNode;
    files?: AttachmentComposerField;
    reply?: ReactNode;
  }) => ReactNode;
};
/** Borrowed refs and immutable render values; the controller constructs and retains them. */
export type MediaComposerRuntime = {
  spaceId: string;
  replyTo: string | null;
  replyContext?: MediaReplyFields;
  captureStage?: RefObject<VoiceCaptureContext | null>;
  setCaptureReview?: Dispatch<SetStateAction<VoiceCaptureContext | null>>;
  thread: boolean;
  threadRootId: string | null;
  disabled: boolean;
  clip: Recording | null;
  captureActive: boolean;
  recorderState: RecorderState;
  suspended: boolean;
  onSent: ((reply?: AcceptedMediaReply) => void) | undefined;
  owners: RefObject<Set<VoiceReviewOwner>>;
  durableOwners: RefObject<Map<VoiceReviewOwner, VoiceDraftOwner>>;
  voiceControlNow: RefObject<object>;
  replaceVoiceControl: (control?: object) => void;
  channel: MediaDraftChannel | null;
  media: MediaDriver | undefined;
  store: ClientStore;
  mounted: RefObject<boolean>;
  setFailure: Dispatch<SetStateAction<string | null>>;
  clipNow: RefObject<Recording | null>;
  uriToDiscard: RefObject<string | undefined>;
  setClip: Dispatch<SetStateAction<Recording | null>>;
  setExpanded: Dispatch<SetStateAction<boolean>>;
  startEpoch: RefObject<{ value: number }>;
  durableState: Readonly<{
    phase: 'loading' | 'ready' | 'closed';
    value: MediaDraftDto | null;
    content: MediaDraftContent | null;
    saving: boolean;
    sending: boolean;
    reviewing: boolean;
    frozen: boolean;
    error: Error | null;
  }> | null;
  restoreAttempt: number;
  levels: RefObject<number[]>;
  setRecordingLevels: Dispatch<SetStateAction<readonly number[]>>;
  stopTask: RefObject<Promise<void> | null>;
  startTask: RefObject<Promise<void> | null>;
  captureControl: RefObject<object | null>;
  recording: RefObject<boolean>;
  prepared: RefObject<boolean>;
  preparedUri: RefObject<string | undefined>;
  audioModeNeedsRestore: RefObject<boolean>;
  stoppedCapture: RefObject<StoppedCapture | null>;
  recorder: AudioRecorder;
  setCaptureActive: Dispatch<SetStateAction<boolean>>;
  setCapturePaused: Dispatch<SetStateAction<boolean>>;
  setRecordingLocked: Dispatch<SetStateAction<boolean>>;
  focused: RefObject<boolean>;
  inform: (title: string, message: string) => Promise<void>;
  getCaptureLease: () => CaptureLease | null;
  captureOwner: {};
  setCaptureLease: (lease: CaptureLease | null) => void;
  playbackRetired: RefObject<boolean>;
  appState: RefObject<string | null | undefined>;
  suspendedNow: RefObject<boolean>;
  disabledNow: RefObject<boolean>;
  busyNow: RefObject<boolean>;
  busy: boolean;
  selecting: RefObject<boolean>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  controlIsCurrent: (control: object) => boolean;
  voiceControl: object;
  trimNow: RefObject<PendingVoiceTrim>;
  setRestoreAttempt: Dispatch<SetStateAction<number>>;
  renderControls:
    | ((controls: {
        attachment: ReactNode;
        voice: ReactNode;
        review?: ReactNode;
        files?: AttachmentComposerField;
        reply?: ReactNode;
      }) => ReactNode)
    | undefined;
  failure: string | null;
  expanded: boolean;
  pendingTrim: PendingVoiceTrim;
  capturePaused: boolean;
  recordingLocked: boolean;
  recordingLevels: readonly number[];
  colors: SemanticColors;
  onPendingTrimChange: (
    session: VoiceSession,
    draft: VoiceDraft,
    pending: boolean,
    previewOwner: object,
  ) => void;
};
