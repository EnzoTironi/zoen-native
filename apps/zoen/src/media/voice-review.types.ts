import type { VoiceSendable, VoiceSession } from './voice-session';

/** The parent owns prepare, capture, cancellation and the native source lease. */
export type VoiceReviewProps = Readonly<{
  session: VoiceSession;
  busy?: boolean;
  initiallyEditing?: boolean;
  onSend: (sendable: VoiceSendable) => Promise<void>;
  onRetryPrepare?: () => void;
}>;
