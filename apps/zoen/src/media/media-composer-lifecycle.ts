import { randomUUID } from 'expo-crypto';
import { AppState, Platform } from 'react-native';
import { t } from '../i18n/index.ts';
import type { useAttachmentComposer } from './attachment-composer';
import { pauseAudioPlayback } from './AudioClip';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import type { MediaComposerRuntime } from './media-composer-contract';
import { discard, failureMessage, type Recording } from './media-composer-shared';
import { VoiceDraftOwner, frozenVoicePreview } from './voice-draft-owner';
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

export function voiceLifecycle1(context: Pick<MediaComposerRuntime, 'uriToDiscard' | 'clip'>) {
  const { uriToDiscard, clip } = context;

  uriToDiscard.current = clip?.owner ? undefined : clip?.uri;
}

export function voiceLifecycle2(
  context: Pick<
    MediaComposerRuntime,
    'mounted' | 'owners' | 'durableOwners' | 'startEpoch' | 'uriToDiscard'
  > &
    Pick<ComposerCallbacks, 'closeOwner'>,
) {
  const { mounted, owners, durableOwners, startEpoch, closeOwner, uriToDiscard } = context;

  mounted.current = true;
  const retainedOwners = owners.current;
  const retainedDurable = durableOwners.current;
  const epoch = startEpoch.current;
  return () => {
    mounted.current = false;
    pauseAudioPlayback();
    ++epoch.value;
    for (const owner of retainedOwners) {
      const durable = retainedDurable.get(owner);
      if (durable) durable.retire();
      else void closeOwner(owner).catch(() => {});
    }
    void discard(uriToDiscard.current);
  };
}

export function voiceLifecycle3(
  context: Pick<MediaComposerRuntime, 'channel' | 'mounted' | 'setFailure'>,
) {
  const { channel, mounted, setFailure } = context;

  if (channel)
    void channel.load().catch((reason: unknown) => {
      if (mounted.current) setFailure(failureMessage(reason));
    });
}

export function voiceLifecycle4(
  context: Pick<
    MediaComposerRuntime,
    | 'channel'
    | 'media'
    | 'clipNow'
    | 'mounted'
    | 'store'
    | 'setFailure'
    | 'owners'
    | 'durableOwners'
    | 'replaceVoiceControl'
    | 'setClip'
    | 'setExpanded'
  > &
    Pick<ComposerCallbacks, 'savedVoice'>,
) {
  const {
    channel,
    savedVoice,
    media,
    clipNow,
    mounted,
    store,
    setFailure,
    owners,
    durableOwners,
    replaceVoiceControl,
    setClip,
    setExpanded,
  } = context;

  if (!channel || !savedVoice || !media?.voiceEditor || clipNow.current) return;
  let active = true;
  const materialized: string[] = [];
  let durable: VoiceDraftOwner | undefined;
  void (async () => {
    try {
      const source = await channel.materialize(savedVoice.source_asset_id);
      materialized.push(source.uri);
      let preview;
      if (channel.getSnapshot().frozen) {
        if (!savedVoice.render) throw new Error(t('media.voiceRecordingUnavailable'));
        const rendered = await channel.materialize(savedVoice.render.asset_id);
        materialized.push(rendered.uri);
        preview = frozenVoicePreview(savedVoice, rendered.uri);
      }
      if (!active || !mounted.current || clipNow.current || !store.mediaDrafts.isCurrent(channel))
        return;
      durable = new VoiceDraftOwner({
        channel,
        port: media.voiceEditor!,
        uri: source.uri,
        id: randomUUID,
        replyTo: savedVoice.reply_to,
        restored: savedVoice,
        frozenPreview: preview,
        registerCleanup: (cleanup) => store.registerDeviceCleanup(cleanup),
        work: sensitiveCacheWork,
        discard: discardCacheCopy,
        releaseMaterialized: async () => {
          for (const uri of materialized) await channel.release(uri);
        },
        onError: (reason) => {
          if (mounted.current) setFailure(failureMessage(reason));
        },
      });
      owners.current.add(durable.owner);
      durableOwners.current.set(durable.owner, durable);
      const value = {
        uri: source.uri,
        owner: durable.owner,
        durable,
        durationMs: savedVoice.render?.duration_ms ?? 0,
        levels: [...(savedVoice.render?.levels ?? [])],
      };
      replaceVoiceControl();
      clipNow.current = value;
      setClip(value);
      await durable.prepare();
      if (active && mounted.current && durable.owner.session.getSnapshot().pendingTrim)
        setExpanded(true);
    } catch (reason) {
      if (active && mounted.current) setFailure(failureMessage(reason));
    } finally {
      if (!durable) await Promise.allSettled(materialized.map((uri) => channel.release(uri)));
    }
  })();
  return () => {
    active = false;
  };
}

export function voiceLifecycle5(
  context: Pick<
    MediaComposerRuntime,
    'clipNow' | 'channel' | 'replaceVoiceControl' | 'setClip' | 'setExpanded'
  >,
) {
  const { clipNow, channel, replaceVoiceControl, setClip, setExpanded } = context;

  const previous = clipNow.current;
  if (previous?.durable && previous.durable.channel !== channel) {
    pauseAudioPlayback();
    clipNow.current = null;
    previous.durable.retire();
    replaceVoiceControl();
    setClip(null);
    setExpanded(false);
  }
}

export function voiceLifecycle6(
  context: Pick<MediaComposerRuntime, 'levels' | 'setRecordingLevels'> & {
    recorderState: Pick<MediaComposerRuntime['recorderState'], 'isRecording' | 'metering'>;
  },
) {
  const { recorderState, levels, setRecordingLevels } = context;

  if (!recorderState.isRecording) return;
  if (typeof recorderState.metering === 'number') {
    levels.current.push(Math.min(1, Math.max(0.02, 10 ** (recorderState.metering / 40))));
    setRecordingLevels(levels.current.slice(-18));
  }
}

export function voiceLifecycle7(
  context: Pick<
    MediaComposerRuntime,
    'store' | 'playbackRetired' | 'mounted' | 'recorder' | 'uriToDiscard' | 'owners'
  > &
    Pick<ComposerCallbacks, 'stop'>,
) {
  const { store, playbackRetired, mounted, recorder, stop, uriToDiscard, owners } = context;

  if (Platform.OS !== 'ios') return;
  return store.registerDeviceCleanup(async () => {
    playbackRetired.current = true;
    pauseAudioPlayback();
    const uri = mounted.current ? recorder.uri : undefined;
    await stop();
    // SDK58 releases this SharedObject on unmount and synchronously stops
    // its native recorder. A retained retry must never call that released object.
    if (mounted.current && recorder.getStatus().isRecording) await recorder.stop();
    if (mounted.current && recorder.getStatus().isRecording)
      throw new Error(t('media.theRecordingOnThisDeviceCouldNotBe'));
    discardCacheCopy(uriToDiscard.current);
    if (![...owners.current].some((owner) => owner.uri === uri)) discardCacheCopy(uri ?? undefined);
  });
}

export function voiceLifecycle8(
  context: Pick<MediaComposerRuntime, 'focused' | 'startEpoch'> & Pick<ComposerCallbacks, 'stop'>,
) {
  const { focused, startEpoch, stop } = context;

  focused.current = true;
  return () => {
    focused.current = false;
    pauseAudioPlayback();
    ++startEpoch.current.value;
    void stop().catch(() => {});
  };
}

export function voiceLifecycle9(
  context: Pick<MediaComposerRuntime, 'appState' | 'startEpoch'> & Pick<ComposerCallbacks, 'stop'>,
) {
  const { appState, startEpoch, stop } = context;

  const subscription = AppState.addEventListener('change', (state) => {
    appState.current = state;
    if (state !== 'active') {
      pauseAudioPlayback();
      ++startEpoch.current.value;
      void stop().catch(() => {});
    }
  });
  return () => subscription.remove();
}

export function voiceLifecycle10(
  context: Pick<
    MediaComposerRuntime,
    'suspendedNow' | 'suspended' | 'disabledNow' | 'disabled' | 'startEpoch'
  > &
    Pick<ComposerCallbacks, 'stop'>,
) {
  const { suspendedNow, suspended, disabledNow, disabled, startEpoch, stop } = context;

  // Search keeps this owner mounted. End a live capture into its retained draft
  // before hiding the recording controls; returning restores the same edits.
  suspendedNow.current = suspended;
  disabledNow.current = disabled;
  if (suspended || disabled) {
    pauseAudioPlayback();
    ++startEpoch.current.value;
    void stop().catch(() => {});
  }
}

export function voiceLifecycle11(
  context: {
    recorderState: Pick<MediaComposerRuntime['recorderState'], 'isRecording' | 'durationMillis'>;
  } & Pick<ComposerCallbacks, 'stop'>,
) {
  const { recorderState, stop } = context;

  if (recorderState.isRecording && recorderState.durationMillis >= 3_600_000)
    void stop().catch(() => {});
}
