import { AudioModule } from 'expo-audio';
import { setMediaAudioMode } from './audio-mode.ts';
import { callAudioFocus } from './call-audio-focus.ts';
import { randomUUID } from 'expo-crypto';
import { t } from '../i18n/index.ts';
import type { useAttachmentComposer } from './attachment-composer';
import { pauseAudioPlayback } from './AudioClip';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import type { MediaComposerRuntime } from './media-composer-contract';
import { discard, failureMessage, type Recording } from './media-composer-shared';
import { VoiceDraftOwner } from './voice-draft-owner';
import { VoiceReviewOwner } from './voice-review-owner';
import { type VoiceSendable } from './voice-session';
import { copyMediaReply } from './media-reply';
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

export function stopCapture(
  context: Pick<
    MediaComposerRuntime,
    | 'stopTask'
    | 'startTask'
    | 'captureControl'
    | 'captureStage'
    | 'setCaptureReview'
    | 'recording'
    | 'prepared'
    | 'preparedUri'
    | 'audioModeNeedsRestore'
    | 'startEpoch'
    | 'mounted'
    | 'stoppedCapture'
    | 'recorder'
    | 'setCaptureActive'
    | 'setCapturePaused'
    | 'setRecordingLocked'
    | 'focused'
    | 'levels'
    | 'inform'
    | 'setFailure'
    | 'getCaptureLease'
    | 'captureOwner'
    | 'setCaptureLease'
    | 'voiceControlNow'
    | 'clipNow'
    | 'replaceVoiceControl'
  > &
    Pick<ComposerCallbacks, 'stageVoice'>,
) {
  const {
    stopTask,
    startTask,
    captureControl,
    captureStage,
    setCaptureReview,
    recording,
    prepared,
    preparedUri,
    audioModeNeedsRestore,
    startEpoch,
    mounted,
    stoppedCapture,
    recorder,
    setCaptureActive,
    setCapturePaused,
    setRecordingLocked,
    focused,
    levels,
    stageVoice,
    inform,
    setFailure,
    getCaptureLease,
    captureOwner,
    setCaptureLease,
    voiceControlNow,
    clipNow,
    replaceVoiceControl,
  } = context;

  if (stopTask.current) return stopTask.current;
  const starting = startTask.current;
  const control = captureControl.current;
  const review = captureStage?.current;
  if (
    !starting &&
    !recording.current &&
    !prepared.current &&
    !preparedUri.current &&
    !audioModeNeedsRestore.current
  )
    return Promise.resolve();
  if (starting) ++startEpoch.current.value;
  // Register before SDK callbacks, and drain an interrupted preparation before
  // restoring the audio mode. A newer capture cannot overtake this cleanup.
  const task = Promise.resolve().then(async () => {
    if (starting) await starting.catch(() => {});
    if (
      !recording.current &&
      !prepared.current &&
      !preparedUri.current &&
      !audioModeNeedsRestore.current
    )
      return;
    audioModeNeedsRestore.current = true;
    let failure: unknown;
    let failed = false;
    try {
      // SDK58 releases/stops this SharedObject on unmount; retained cleanup must not touch it.
      if (!mounted.current) {
        recording.current = false;
        prepared.current = false;
        preparedUri.current = undefined;
      }
      if (recording.current) {
        const capture = (stoppedCapture.current ??= {
          uri: recorder.uri ?? undefined,
          durationMs: recorder.getStatus().durationMillis,
          stopped: false,
        });
        // iOS can pause the recorder before AppState reaches JS. isRecording
        // false does not mean its M4A is finalized; SDK58 stop also closes paused audio.
        if (!capture.stopped) {
          capture.durationMs = Math.max(capture.durationMs, recorder.getStatus().durationMillis);
          await recorder.stop();
          capture.stopped = true;
        }
        // Web publishes its recording URI only after stop. Read the final URI
        // while this SharedObject is still mounted, retaining it across retries.
        if (mounted.current && !capture.terminal) capture.uri = recorder.uri ?? capture.uri;
        if (mounted.current && recorder.getStatus().isRecording) {
          capture.stopped = false;
          throw new Error(t('media.theRecordingOnThisDeviceCouldNotBe'));
        }
        // A failed stop retains capture ownership and can be retried; never discard live audio.
        recording.current = false;
        if (mounted.current) {
          setCaptureActive(false);
          setCapturePaused(false);
          setRecordingLocked(false);
        }
        prepared.current = false;
        preparedUri.current = undefined;
        const { durationMs } = capture;
        const uri = capture.uri;
        if (uri && (durationMs >= 250 || capture.terminal) && mounted.current && focused.current) {
          const stride = Math.max(1, Math.ceil(levels.current.length / 160));
          (review?.stage ?? stageVoice)(
            {
              uri,
              durationMs: Math.round(durationMs),
              levels: levels.current.filter((_, index) => index % stride === 0),
            },
            true,
            control ?? undefined,
          );
          if (captureStage && captureStage.current === review) captureStage.current = null;
          setCaptureReview?.((current) => (current === review ? null : current));
        } else {
          await discard(uri ?? undefined);
          if (mounted.current && focused.current && durationMs < 250)
            void inform(t('media.recordingTooShort'), t('media.recordForAtLeastOneSecondToSend'));
        }
        if (capture.replacementUri) await discard(capture.replacementUri);
        stoppedCapture.current = null;
      } else if ((prepared.current || preparedUri.current) && mounted.current) {
        if (prepared.current) await recorder.stop();
        if (mounted.current) preparedUri.current = recorder.uri ?? preparedUri.current;
        await discard(preparedUri.current);
        prepared.current = false;
        preparedUri.current = undefined;
      }
    } catch (reason) {
      failure = reason;
      failed = true;
    }
    try {
      await setMediaAudioMode({ allowsRecording: false });
      audioModeNeedsRestore.current = false;
    } catch (reason) {
      if (!failed) failure = reason;
      failed = true;
    }
    if (failed) {
      if (mounted.current) setFailure(failureMessage(failure));
      throw failure;
    }
  });
  stopTask.current = task;
  // Releasing the single-flight latch must not depend on successful audio-session restoration.
  void task
    .finally(() => {
      if (stopTask.current === task) stopTask.current = null;
      if (
        getCaptureLease()?.owner === captureOwner &&
        !recording.current &&
        !prepared.current &&
        !preparedUri.current &&
        !audioModeNeedsRestore.current
      )
        setCaptureLease(null);
      if (
        control &&
        voiceControlNow.current === control &&
        !recording.current &&
        !clipNow.current &&
        !audioModeNeedsRestore.current
      )
        replaceVoiceControl();
    })
    .catch(() => {});
  return sensitiveCacheWork.track(task);
}

export function startCapture(
  context: Pick<
    MediaComposerRuntime,
    | 'controlIsCurrent'
    | 'voiceControl'
    | 'busy'
    | 'startTask'
    | 'stopTask'
    | 'recording'
    | 'clipNow'
    | 'disabledNow'
    | 'suspendedNow'
    | 'focused'
    | 'appState'
    | 'channel'
    | 'captureControl'
    | 'captureStage'
    | 'setRecordingLocked'
    | 'setCaptureReview'
    | 'replyTo'
    | 'replyContext'
    | 'startEpoch'
    | 'setBusy'
    | 'setFailure'
    | 'getCaptureLease'
    | 'mounted'
    | 'setCaptureLease'
    | 'captureOwner'
    | 'prepared'
    | 'preparedUri'
    | 'audioModeNeedsRestore'
    | 'voiceControlNow'
    | 'replaceVoiceControl'
  > &
    Pick<ComposerCallbacks, 'stop' | 'startRecording' | 'stageVoice'>,
  held = false,
): Promise<boolean> {
  const {
    controlIsCurrent,
    voiceControl,
    busy,
    startTask,
    stopTask,
    recording,
    clipNow,
    disabledNow,
    suspendedNow,
    focused,
    appState,
    channel,
    captureControl,
    captureStage,
    stageVoice,
    setCaptureReview,
    replyTo,
    replyContext,
    setRecordingLocked,
    startEpoch,
    setBusy,
    setFailure,
    getCaptureLease,
    mounted,
    setCaptureLease,
    captureOwner,
    stop,
    prepared,
    preparedUri,
    audioModeNeedsRestore,
    voiceControlNow,
    replaceVoiceControl,
    startRecording,
  } = context;

  if (
    !callAudioFocus.available ||
    !controlIsCurrent(voiceControl) ||
    busy ||
    startTask.current ||
    stopTask.current ||
    recording.current ||
    clipNow.current ||
    disabledNow.current ||
    suspendedNow.current ||
    !focused.current ||
    appState.current !== 'active' ||
    (channel && (channel.getSnapshot().phase !== 'ready' || channel.getSnapshot().content !== null))
  )
    return Promise.resolve(false);
  captureControl.current = voiceControl;
  if (captureStage)
    captureStage.current = Object.freeze({
      channel,
      stage: stageVoice,
      replyTo,
      replyContext: copyMediaReply(replyContext ?? {}),
    });
  if (captureStage?.current) setCaptureReview?.(captureStage.current);
  setRecordingLocked(!held);
  const epoch = startEpoch.current.value;
  setBusy(true);
  setFailure(null);
  // Capture recovery before assigning this attempt: its stop must never await
  // the new start that is itself waiting for that old cleanup.
  const recovery = getCaptureLease()?.recover();
  const task = sensitiveCacheWork.run(() =>
    Promise.resolve().then(async () => {
      await recovery;
      if (
        !callAudioFocus.available ||
        epoch !== startEpoch.current.value ||
        !mounted.current ||
        getCaptureLease()
      )
        return;
      setCaptureLease({ owner: captureOwner, recover: stop });
      await startRecording(epoch);
    }),
  );
  startTask.current = task;
  void task
    .catch((reason: unknown) => {
      if (mounted.current) setFailure(failureMessage(reason));
    })
    .finally(() => {
      if (startTask.current === task) {
        startTask.current = null;
        if (mounted.current) setBusy(false);
      }
      if (
        getCaptureLease()?.owner === captureOwner &&
        !recording.current &&
        !prepared.current &&
        !preparedUri.current &&
        !audioModeNeedsRestore.current
      )
        setCaptureLease(null);
      if (
        voiceControlNow.current === voiceControl &&
        !recording.current &&
        !clipNow.current &&
        !audioModeNeedsRestore.current
      )
        replaceVoiceControl();
    });
  return task.then(
    () => recording.current,
    () => false,
  );
}

export async function prepareCapture(
  context: Pick<
    MediaComposerRuntime,
    | 'startEpoch'
    | 'getCaptureLease'
    | 'captureOwner'
    | 'mounted'
    | 'focused'
    | 'appState'
    | 'disabledNow'
    | 'suspendedNow'
    | 'media'
    | 'recording'
    | 'stopTask'
    | 'audioModeNeedsRestore'
    | 'clipNow'
    | 'inform'
    | 'recorder'
    | 'prepared'
    | 'preparedUri'
    | 'levels'
    | 'setRecordingLevels'
    | 'setExpanded'
    | 'setCaptureActive'
    | 'setCapturePaused'
    | 'stoppedCapture'
    | 'uriToDiscard'
  > &
    Pick<ComposerCallbacks, 'attachments'>,
  epoch: number,
) {
  const {
    startEpoch,
    getCaptureLease,
    captureOwner,
    mounted,
    focused,
    appState,
    disabledNow,
    suspendedNow,
    media,
    attachments,
    recording,
    stopTask,
    audioModeNeedsRestore,
    clipNow,
    inform,
    recorder,
    prepared,
    preparedUri,
    levels,
    setRecordingLevels,
    setExpanded,
    setCaptureActive,
    setCapturePaused,
    stoppedCapture,
    uriToDiscard,
  } = context;

  const current = () =>
    callAudioFocus.available &&
    epoch === startEpoch.current.value &&
    getCaptureLease()?.owner === captureOwner &&
    mounted.current &&
    focused.current &&
    appState.current === 'active' &&
    !disabledNow.current &&
    !suspendedNow.current;
  if (
    !media ||
    attachments.active ||
    attachments.busy ||
    !current() ||
    recording.current ||
    stopTask.current ||
    audioModeNeedsRestore.current ||
    clipNow.current
  )
    return;
  let started = false;
  try {
    const permission = await AudioModule.requestRecordingPermissionsAsync();
    if (!current()) return;
    if (!permission.granted) {
      await inform(
        t('media.allowMicrophone'),
        t('media.allowTheMicrophoneInDeviceSettingsToRecord'),
      );
      return;
    }
    pauseAudioPlayback();
    audioModeNeedsRestore.current = true;
    await setMediaAudioMode({
      playsInSilentMode: true,
      allowsRecording: true,
      shouldPlayInBackground: false,
      allowsBackgroundRecording: false,
      interruptionMode: 'doNotMix',
    });
    if (!current()) return;
    try {
      await recorder.prepareToRecordAsync();
      if (mounted.current) prepared.current = true;
    } finally {
      if (mounted.current) preparedUri.current = recorder.uri ?? undefined;
    }
    if (!current()) return;
    levels.current = [];
    setRecordingLevels([]);
    setExpanded(false);
    recorder.record();
    recording.current = true;
    setCaptureActive(true);
    setCapturePaused(false);
    stoppedCapture.current = { uri: recorder.uri ?? undefined, durationMs: 0, stopped: false };
    started = true;
    uriToDiscard.current = recorder.uri ?? undefined;
  } catch (reason) {
    if (current()) void inform(t('media.couldNotRecord'), failureMessage(reason));
  } finally {
    if (!started && audioModeNeedsRestore.current) {
      // An unmounted SDK SharedObject is already stopped/released. Never read it
      // from a late prepare continuation; the remaining session restore is independent.
      if (mounted.current && (prepared.current || preparedUri.current)) {
        if (prepared.current) await recorder.stop();
        if (mounted.current) preparedUri.current = recorder.uri ?? preparedUri.current;
        await discard(preparedUri.current);
        prepared.current = false;
        preparedUri.current = undefined;
      }
      await setMediaAudioMode({ allowsRecording: false });
      audioModeNeedsRestore.current = false;
    }
  }
}

export async function toggleCapturePause(
  context: Pick<
    MediaComposerRuntime,
    | 'controlIsCurrent'
    | 'voiceControl'
    | 'busyNow'
    | 'recording'
    | 'stopTask'
    | 'startTask'
    | 'mounted'
    | 'focused'
    | 'disabledNow'
    | 'suspendedNow'
    | 'appState'
    | 'getCaptureLease'
    | 'captureOwner'
    | 'recorder'
    | 'setCapturePaused'
    | 'setRecordingLocked'
    | 'setFailure'
  >,
) {
  const {
    controlIsCurrent,
    voiceControl,
    busyNow,
    recording,
    stopTask,
    startTask,
    mounted,
    focused,
    disabledNow,
    suspendedNow,
    appState,
    getCaptureLease,
    captureOwner,
    recorder,
    setCapturePaused,
    setRecordingLocked,
    setFailure,
  } = context;

  // Pause/resume use the same prepared recorder and lease. A paused M4A still needs Stop.
  if (
    !controlIsCurrent(voiceControl) ||
    busyNow.current ||
    !recording.current ||
    stopTask.current ||
    startTask.current ||
    !mounted.current ||
    !focused.current ||
    disabledNow.current ||
    suspendedNow.current ||
    appState.current !== 'active' ||
    getCaptureLease()?.owner !== captureOwner
  )
    return;
  try {
    if (recorder.getStatus().isRecording) {
      recorder.pause();
      setCapturePaused(true);
    } else {
      recorder.record();
      setCapturePaused(false);
    }
    setRecordingLocked(true);
  } catch (reason) {
    if (mounted.current) setFailure(failureMessage(reason));
  }
}

export function stageVoice(
  context: Pick<
    MediaComposerRuntime,
    | 'voiceControlNow'
    | 'replaceVoiceControl'
    | 'channel'
    | 'media'
    | 'replyTo'
    | 'replyContext'
    | 'store'
    | 'mounted'
    | 'setFailure'
    | 'owners'
    | 'durableOwners'
    | 'clipNow'
    | 'uriToDiscard'
    | 'setClip'
    | 'setExpanded'
  >,
  capture: Omit<Recording, 'owner'>,
  preserveExpansion = false,
  control?: object,
) {
  const {
    replyTo,
    replyContext,
    voiceControlNow,
    replaceVoiceControl,
    channel,
    media,
    store,
    mounted,
    setFailure,
    owners,
    durableOwners,
    clipNow,
    uriToDiscard,
    setClip,
    setExpanded,
  } = context;

  if (control && control !== voiceControlNow.current) {
    void discard(capture.uri);
    return;
  }
  replaceVoiceControl(control);
  const durable =
    channel && media?.voiceEditor
      ? new VoiceDraftOwner({
          channel,
          port: media.voiceEditor,
          uri: capture.uri,
          replyTo,
          replyQuote: replyContext?.reply_quote,
          sharedReply: replyContext?.shared_reply,
          replyIntent: replyContext?.reply_intent,
          id: randomUUID,
          registerCleanup: (cleanup) => store.registerDeviceCleanup(cleanup),
          work: sensitiveCacheWork,
          discard: discardCacheCopy,
          onError: (reason) => {
            if (mounted.current) setFailure(failureMessage(reason));
          },
        })
      : undefined;
  const owner =
    durable?.owner ??
    (media?.voiceEditor
      ? new VoiceReviewOwner({
          port: media.voiceEditor,
          uri: capture.uri,
          registerCleanup: (cleanup) => store.registerDeviceCleanup(cleanup),
          work: sensitiveCacheWork,
          discard: discardCacheCopy,
        })
      : undefined);
  if (owner) owners.current.add(owner);
  if (owner && durable) durableOwners.current.set(owner, durable);
  const value = { ...capture, owner, durable };
  clipNow.current = value;
  uriToDiscard.current = owner ? undefined : capture.uri;
  setClip(value);
  if (!preserveExpansion) setExpanded(false);
  if (owner)
    void (durable ? durable.prepare() : owner.prepare()).catch((reason: unknown) => {
      if (mounted.current) setFailure(failureMessage(reason));
    });
}
