import { Platform } from 'react-native';
/** Explicit publication keeps the same owner, Core receipt and native control fences. */
import { t } from '../i18n/index.ts';
import { pauseAudioPlayback } from './AudioClip';
import { discard, failureMessage } from './media-composer-shared';
import { hasPendingVoiceTrim } from './voice-trim-preview';
import { mediaAcceptedIntent, type AcceptedMediaReply } from './media-acceptance';
import type { MediaComposerRuntime } from './media-composer-contract';
import type { VoiceReviewOwner } from './voice-review-owner';
import type { VoiceSendable } from './voice-session';
export type VoicePublicationContext = Pick<
  MediaComposerRuntime,
  | 'controlIsCurrent'
  | 'voiceControl'
  | 'media'
  | 'busyNow'
  | 'clipNow'
  | 'trimNow'
  | 'setBusy'
  | 'setFailure'
  | 'durableOwners'
  | 'spaceId'
  | 'replyTo'
  | 'thread'
  | 'voiceControlNow'
  | 'store'
  | 'replaceVoiceControl'
  | 'setClip'
  | 'uriToDiscard'
  | 'onSent'
  | 'mounted'
  | 'clip'
> & {
  voiceIsActive(): boolean;
  stop(): Promise<void>;
  closeOwner(owner: VoiceReviewOwner): Promise<void>;
};
export async function sendEdited(
  context: VoicePublicationContext,
  owner: VoiceReviewOwner,
  captured: VoiceSendable,
) {
  const {
    controlIsCurrent,
    voiceControl,
    media,
    busyNow,
    clipNow,
    trimNow,
    setBusy,
    setFailure,
    durableOwners,
    spaceId,
    replyTo,
    thread,
    voiceControlNow,
    store,
    replaceVoiceControl,
    setClip,
    uriToDiscard,
    onSent,
    mounted,
    voiceIsActive,
    stop,
    closeOwner,
  } = context;
  // Like raw Send, lifecycle retirement cancels quietly without consuming the draft.
  if (!voiceIsActive() || !controlIsCurrent(voiceControl)) return;
  if (!media || busyNow.current) throw new Error(t('media.voiceRecordingUnavailable'));
  if (clipNow.current?.owner !== owner || hasPendingVoiceTrim(trimNow.current, owner.session))
    throw new Error(t('media.voiceRecordingUnavailable'));
  busyNow.current = true;
  pauseAudioPlayback();
  setBusy(true);
  setFailure(null);
  try {
    await stop();
    if (!voiceIsActive() || !controlIsCurrent(voiceControl)) return;
    if (clipNow.current?.owner !== owner || hasPendingVoiceTrim(trimNow.current, owner.session))
      throw new Error(t('media.voiceRecordingUnavailable'));
    const durable = durableOwners.current.get(owner);
    let acceptedReply: AcceptedMediaReply | undefined;
    if (durable) acceptedReply = mediaAcceptedIntent((await durable.send(captured)).draft)?.reply;
    else
      await owner.send(captured, (value) =>
        media.sendVoice({
          uri: value.preview.uri,
          durationMs: value.preview.durationMs,
          levels: [...value.preview.levels],
          mime: value.preview.mime,
          transcript: value.transcript,
          voiceSourceId: owner.session.getSnapshot().source?.id,
          spaceId,
          replyTo,
          thread,
        }),
      );
    // Acknowledged once: cleanup/refresh failures cannot resubmit the mutation.
    if (voiceControlNow.current !== voiceControl) {
      void closeOwner(owner).catch(() => {});
      void store.refresh().catch(() => {});
      return;
    }
    replaceVoiceControl();
    clipNow.current = null;
    setClip(null);
    uriToDiscard.current = undefined;
    void closeOwner(owner).catch(() => {});
    onSent?.(acceptedReply);
    void store.refresh().catch(() => {});
  } finally {
    busyNow.current = false;
    if (mounted.current) setBusy(false);
  }
}
export async function send(context: VoicePublicationContext) {
  const {
    controlIsCurrent,
    voiceControl,
    media,
    busyNow,
    clipNow,
    setBusy,
    setFailure,
    spaceId,
    replyTo,
    thread,
    voiceControlNow,
    store,
    replaceVoiceControl,
    setClip,
    uriToDiscard,
    onSent,
    mounted,
    clip,
    voiceIsActive,
    stop,
  } = context;
  if (
    !controlIsCurrent(voiceControl) ||
    !voiceIsActive() ||
    !media ||
    busyNow.current ||
    !clip ||
    clip.owner
  )
    return;
  busyNow.current = true;
  setBusy(true);
  setFailure(null);
  const uri = clip.uri;
  try {
    await stop();
    if (!controlIsCurrent(voiceControl) || !voiceIsActive()) return;
    await media.sendVoice({
      uri: clip.uri,
      durationMs: clip.durationMs,
      levels: clip.levels,
      spaceId,
      mime: Platform.OS === 'web' ? 'audio/webm' : 'audio/mp4',
      replyTo,
      thread,
    });
    if (voiceControlNow.current !== voiceControl) {
      void discard(uri).catch(() => {});
      void store.refresh().catch(() => {});
      return;
    }
    replaceVoiceControl();
    clipNow.current = null;
    setClip(null);
    uriToDiscard.current = undefined;
    onSent?.();
    // Acknowledged sends stay successful even if local cache cleanup needs a retry.
    void discard(uri).catch(() => {});
    void store.refresh().catch(() => {});
  } catch (reason) {
    setFailure(failureMessage(reason));
  } finally {
    busyNow.current = false;
    if (mounted.current) setBusy(false);
  }
}
