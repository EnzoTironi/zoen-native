import type { VoiceSendable, VoiceSession } from './voice-session';
import type { VoiceDraft } from './voice-edit';

/** The parent owns prepare, capture, cancellation and the native source lease. */
export type VoiceReviewProps = Readonly<{
  session: VoiceSession;
  busy?: boolean;
  initiallyEditing?: boolean;
  onSend: (sendable: VoiceSendable) => Promise<void>;
  onRetryPrepare?: () => void;
  onPendingTrimChange?: (
    session: VoiceSession,
    draft: VoiceDraft,
    pending: boolean,
    previewOwner: object,
  ) => void;
}>;
