import * as DocumentPicker from 'expo-document-picker';
import { t } from '../i18n/index.ts';
import type { useAttachmentComposer } from './attachment-composer';
import type { MediaPickKind } from './media-actions.types';
import type { MediaComposerRuntime } from './media-composer-contract';
import { discard, failureMessage, type Recording } from './media-composer-shared';
import type { VoiceReviewOwner } from './voice-review-owner';
import { type VoiceSendable } from './voice-session';
type ComposerCallbacks = {
  savedVoice: Extract<
    NonNullable<MediaComposerRuntime['durableState']>['content'],
    { kind: 'voice' }
  > | null;
  attachments: ReturnType<typeof useAttachmentComposer>;
  stageVoice: (
    capture: Omit<Recording, 'owner'>,
    preserveExpansion?: boolean,
    control?: object,
  ) => void;
  closeOwner: (owner: VoiceReviewOwner) => Promise<void>;
  stop: () => Promise<void>;
  startRecording: (epoch: number) => Promise<void>;
  cancel: () => Promise<void>;
  sendEdited: (owner: VoiceReviewOwner, captured: VoiceSendable) => Promise<void>;
};

export async function pickSelection(
  context: Pick<
    MediaComposerRuntime,
    | 'media'
    | 'busy'
    | 'disabled'
    | 'suspendedNow'
    | 'selecting'
    | 'focused'
    | 'setBusy'
    | 'setFailure'
    | 'mounted'
    | 'inform'
  > &
    Pick<ComposerCallbacks, 'attachments' | 'stageVoice'>,
  kind: MediaPickKind,
) {
  const {
    media,
    busy,
    disabled,
    suspendedNow,
    selecting,
    focused,
    attachments,
    setBusy,
    setFailure,
    mounted,
    stageVoice,
    inform,
  } = context;

  if (!media || busy || disabled || suspendedNow.current || selecting.current || !focused.current)
    return;
  if (kind !== 'audio') {
    await attachments.pick(kind);
    return;
  }
  if (attachments.active || attachments.busy) return;
  selecting.current = true;
  setBusy(true);
  setFailure(null);
  try {
    const result = await DocumentPicker.getDocumentAsync({
      copyToCacheDirectory: true,
      multiple: false,
      type: 'audio/*',
      base64: false,
    });
    if (!result.canceled) {
      const file = result.assets[0];
      if (file) {
        if (!mounted.current || !focused.current) await discard(file.uri);
        else stageVoice({ uri: file.uri, durationMs: 0, levels: [] });
      }
    }
  } catch (reason) {
    if (mounted.current && focused.current)
      await inform(t('media.theMediaCouldNotBeSelected'), failureMessage(reason));
  } finally {
    selecting.current = false;
    if (mounted.current) setBusy(false);
  }
}

export async function retryVoice(
  context: Pick<
    MediaComposerRuntime,
    | 'controlIsCurrent'
    | 'voiceControl'
    | 'getCaptureLease'
    | 'captureOwner'
    | 'clipNow'
    | 'channel'
    | 'mounted'
    | 'focused'
    | 'setFailure'
    | 'setRestoreAttempt'
  > &
    Pick<ComposerCallbacks, 'stop' | 'sendEdited' | 'cancel'>,
) {
  const {
    controlIsCurrent,
    voiceControl,
    getCaptureLease,
    captureOwner,
    stop,
    clipNow,
    channel,
    mounted,
    focused,
    setFailure,
    setRestoreAttempt,
    sendEdited,
    cancel,
  } = context;

  if (!controlIsCurrent(voiceControl)) return;
  // Restore the audio session first, without throwing away a successfully staged capture.
  const lease = getCaptureLease();
  if (lease?.owner !== captureOwner) await lease?.recover();
  if (!controlIsCurrent(voiceControl)) return;
  await stop();
  if (!controlIsCurrent(voiceControl)) return;
  const capturedClip = clipNow.current;
  if (!capturedClip && channel) {
    await channel.retry();
    if (mounted.current && focused.current) {
      setFailure(null);
      setRestoreAttempt((value) => value + 1);
    }
    return;
  }
  if (capturedClip?.durable?.channel.getSnapshot().frozen) {
    const captured = capturedClip.owner?.session.getSendable();
    if (!captured || !capturedClip.owner) throw new Error(t('media.voiceRecordingUnavailable'));
    await sendEdited(capturedClip.owner, captured);
    return;
  }
  if (capturedClip?.durable) await capturedClip.durable.channel.retry();
  if (!controlIsCurrent(voiceControl) || !focused.current) return;
  setFailure(null);
  const owner = clipNow.current?.owner;
  if (!owner) return;
  const current = owner.session.getSnapshot();
  if (current.phase === 'closing' || current.phase === 'closed') await cancel();
  else if (current.phase === 'ready' && current.error && current.error.kind !== 'transcribe')
    await owner.session.retryPreview();
  else if (current.phase === 'idle') await (clipNow.current?.durable?.prepare() ?? owner.prepare());
}
