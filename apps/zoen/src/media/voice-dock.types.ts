import type { VoiceSendable, VoiceSession } from './voice-session';

export type VoiceDockProps = Readonly<{
  session?: VoiceSession;
  recording: boolean;
  uri?: string;
  durationMs: number;
  levels: readonly number[];
  busy: boolean;
  failure: string | null;
  onCancel: () => Promise<void>;
  onStop: () => Promise<void>;
  onExpand: () => void;
  onRetry?: () => Promise<void>;
  onSend: (captured?: VoiceSendable) => Promise<void>;
}>;
