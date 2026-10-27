import { Platform } from 'react-native';
import { t } from '../i18n/index.ts';
import { discardCacheCopy } from './cache-files';
import type { VoiceDraftOwner } from './voice-draft-owner';
import type { VoiceReviewOwner } from './voice-review-owner';
import { VoiceSessionFailure } from './voice-session';

export type StoppedCapture = {
  uri: string | undefined;
  durationMs: number;
  stopped: boolean;
  terminal?: boolean;
  replacementUri?: string;
};
export type Recording = {
  uri: string;
  durationMs: number;
  levels: number[];
  owner?: VoiceReviewOwner;
  durable?: VoiceDraftOwner;
};
export const failureMessage = (reason: unknown) => {
  if (reason instanceof VoiceSessionFailure) {
    if (reason.kind === 'cleanup') return t('media.voiceCleanupFailed');
    if (reason.kind === 'prepare') return t('media.voicePrepareFailed');
    if (reason.kind === 'transcribe') return t('media.voiceTranscriptionFailed');
    if (reason.kind === 'closed' || reason.kind === 'not_ready')
      return t('media.voiceRecordingUnavailable');
    return t('media.voicePreviewFailed');
  }
  return reason instanceof Error ? reason.message : t('media.thisCouldNotBeCompletedTryAgain');
};
export async function discard(uri: string | undefined) {
  if (Platform.OS === 'web') {
    if (uri?.startsWith('blob:')) URL.revokeObjectURL(uri);
    return;
  }
  try {
    discardCacheCopy(uri);
  } catch {
    /* Cache eviction also disposes a staged selection. */
  }
}
