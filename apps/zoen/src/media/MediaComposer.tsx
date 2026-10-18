import { t } from '../i18n/index.ts';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { AppState, Platform, ScrollView, Text, View } from 'react-native';
import { useFocusEffect } from 'expo-router';
import * as DocumentPicker from 'expo-document-picker';
import { randomUUID } from 'expo-crypto';
import {
  AudioModule,
  RecordingPresets,
  setAudioModeAsync,
  useAudioRecorder,
  useAudioRecorderState,
} from 'expo-audio';
import { useTheme } from '../theme';
import { IconButton, NativeButton } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useClient } from '../core/provider';
import { AudioClip, clipTime, pauseAudioPlayback } from './AudioClip';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import { ComposerAction } from '../components/composer-action';
import { VoiceReviewOwner } from './voice-review-owner';
import { VoiceDraftOwner, frozenVoicePreview } from './voice-draft-owner';
import { VoiceSessionFailure, type VoiceSendable, type VoiceSession } from './voice-session';
import type { VoiceDraft } from './voice-edit';
import {
  hasPendingVoiceTrim,
  updatePendingVoiceTrim,
  type PendingVoiceTrim,
} from './voice-trim-preview';
import { VoiceReview } from './voice-review';
import { VoiceDock } from './voice-dock';
import { MediaSheet } from './media-sheet';
import { MediaActions } from './media-actions';
import type { MediaPickKind } from './media-actions.types';
import { useAttachmentComposer, type AttachmentComposerField } from './attachment-composer';

type Recording = {
  uri: string;
  durationMs: number;
  levels: number[];
  owner?: VoiceReviewOwner;
  durable?: VoiceDraftOwner;
};
const subscribeNoDraft = () => () => {};
const noDraft = () => null;
// A departing route retains the audio session until its preparation/restore
// drains. A newly mounted composer must not be interrupted by that old restore.
type CaptureLease = { owner: object; recover: () => Promise<void> };
let nativeCaptureLease: CaptureLease | null = null;
const failureMessage = (reason: unknown) => {
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
async function discard(uri: string | undefined) {
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

/** Explicit record → review → send. An interrupted recording is never posted. */
export function MediaComposer({
  spaceId,
  replyTo = null,
  thread = false,
  threadRootId = null,
  disabled = false,
  suspended = false,
  onSent,
  renderControls,
}: {
  spaceId: string;
  replyTo?: string | null;
  thread?: boolean;
  threadRootId?: string | null;
  disabled?: boolean;
  suspended?: boolean;
  onSent?: () => void;
  renderControls?: (controls: {
    attachment: ReactNode;
    voice: ReactNode;
    review?: ReactNode;
    files?: AttachmentComposerField;
  }) => ReactNode;
}) {
  const client = useClient();
  const { store } = client;
  const channel = useMemo(() => {
    if (client.phase !== 'ready' || client.mode !== 'account' || !client.snapshot?.account)
      return null;
    return store.mediaDrafts?.channel(spaceId, threadRootId) ?? null;
  }, [store, spaceId, threadRootId, client.phase, client.mode, client.snapshot]);
  const durableState = useSyncExternalStore(
    channel?.subscribe ?? subscribeNoDraft,
    channel?.getSnapshot ?? noDraft,
    channel?.getSnapshot ?? noDraft,
  );
  const media = store.driver.media;
  const { colors } = useTheme();
  const { inform } = useDialogs();
  const recorder = useAudioRecorder({
    ...RecordingPresets.HIGH_QUALITY,
    numberOfChannels: 1,
    bitRate: 64000,
    isMeteringEnabled: true,
  });
  const recorderState = useAudioRecorderState(recorder, 100);
  const [captureOwner] = useState(() => ({}));
  const webCaptureLease = useRef<CaptureLease | null>(null);
  const getCaptureLease = useCallback(
    () => (Platform.OS === 'web' ? webCaptureLease.current : nativeCaptureLease),
    [],
  );
  const setCaptureLease = useCallback((lease: CaptureLease | null) => {
    // Web has independent MediaRecorders and no shared native audio mode. A
    // failed Web preparation must not lock microphones in future route instances.
    if (Platform.OS === 'web') webCaptureLease.current = lease;
    else nativeCaptureLease = lease;
  }, []);
  const [expanded, setExpanded] = useState(false);
  const [clip, setClip] = useState<Recording | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [restoreAttempt, setRestoreAttempt] = useState(0);
  const levels = useRef<number[]>([]);
  const [recordingLevels, setRecordingLevels] = useState<readonly number[]>([]);
  const stopTask = useRef<Promise<void> | null>(null);
  const startTask = useRef<Promise<void> | null>(null);
  const startEpoch = useRef({ value: 0 });
  const appState = useRef(AppState.currentState);
  const disabledNow = useRef(disabled);
  const prepared = useRef(false);
  const preparedUri = useRef<string | undefined>(undefined);
  const stoppedCapture = useRef<{
    uri: string | undefined;
    durationMs: number;
    stopped: boolean;
  } | null>(null);
  const recording = useRef(false);
  const audioModeNeedsRestore = useRef(false);
  const trimNow = useRef<PendingVoiceTrim>(null);
  const [pendingTrim, setPendingTrim] = useState<PendingVoiceTrim>(null);
  const suspendedNow = useRef(suspended);
  const uriToDiscard = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  const focused = useRef(false);
  const selecting = useRef(false);
  const busyNow = useRef(false);
  const clipNow = useRef<Recording | null>(null);
  const owners = useRef(new Set<VoiceReviewOwner>());
  const durableOwners = useRef(new Map<VoiceReviewOwner, VoiceDraftOwner>());
  const onPendingTrimChange = useCallback(
    (session: VoiceSession, draft: VoiceDraft, pending: boolean, previewOwner: object) => {
      if (pending && clipNow.current?.owner?.session !== session) return;
      const next = updatePendingVoiceTrim(trimNow.current, session, draft, pending, previewOwner);
      if (next === trimNow.current) return;
      trimNow.current = next;
      if (mounted.current) setPendingTrim(next);
    },
    [],
  );
  const attachments = useAttachmentComposer({
    spaceId,
    replyTo,
    thread,
    threadRootId,
    disabled: disabled || Boolean(clip) || recorderState.isRecording,
    suspended,
    onSent,
  });
  const closeOwner = useCallback(async (owner: VoiceReviewOwner) => {
    await owner.close();
    owners.current.delete(owner);
    durableOwners.current.delete(owner);
  }, []);
  const stageVoice = useCallback(
    (capture: Omit<Recording, 'owner'>, preserveExpansion = false) => {
      const durable =
        channel && media?.voiceEditor
          ? new VoiceDraftOwner({
              channel,
              port: media.voiceEditor,
              uri: capture.uri,
              replyTo,
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
    },
    [media, store, channel, replyTo],
  );
  useEffect(() => {
    uriToDiscard.current = clip?.owner ? undefined : clip?.uri;
  }, [clip]);
  useEffect(() => {
    mounted.current = true;
    const retainedOwners = owners.current;
    const retainedDurable = durableOwners.current;
    const epoch = startEpoch.current;
    return () => {
      mounted.current = false;
      ++epoch.value;
      for (const owner of retainedOwners) {
        const durable = retainedDurable.get(owner);
        if (durable) durable.retire();
        else void closeOwner(owner).catch(() => {});
      }
      void discard(uriToDiscard.current);
    };
  }, [closeOwner]);
  useEffect(() => {
    if (channel)
      void channel.load().catch((reason: unknown) => {
        if (mounted.current) setFailure(failureMessage(reason));
      });
  }, [channel]);
  const savedVoice = durableState?.content?.kind === 'voice' ? durableState.content : null;
  useEffect(() => {
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
  }, [channel, savedVoice, media, store, restoreAttempt]);
  useLayoutEffect(() => {
    const previous = clipNow.current;
    if (previous?.durable && previous.durable.channel !== channel) {
      previous.durable.retire();
      clipNow.current = null;
      setClip(null);
      setExpanded(false);
    }
  }, [channel]);
  useEffect(() => {
    if (!recorderState.isRecording) return;
    if (typeof recorderState.metering === 'number') {
      levels.current.push(Math.min(1, Math.max(0.02, 10 ** (recorderState.metering / 40))));
      setRecordingLevels(levels.current.slice(-18));
    }
  }, [recorderState.isRecording, recorderState.durationMillis, recorderState.metering]);

  const stop = useCallback(() => {
    if (stopTask.current) return stopTask.current;
    const starting = startTask.current;
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
            await recorder.stop();
            capture.stopped = true;
          }
          // Web publishes its recording URI only after stop. Read the final URI
          // while this SharedObject is still mounted, retaining it across retries.
          if (mounted.current) capture.uri = recorder.uri ?? capture.uri;
          if (mounted.current && recorder.getStatus().isRecording) {
            capture.stopped = false;
            throw new Error(t('media.theRecordingOnThisDeviceCouldNotBe'));
          }
          // A failed stop retains capture ownership and can be retried; never discard live audio.
          recording.current = false;
          prepared.current = false;
          preparedUri.current = undefined;
          const { durationMs } = capture;
          const uri = capture.uri;
          if (uri && durationMs >= 250 && mounted.current && focused.current) {
            const stride = Math.max(1, Math.ceil(levels.current.length / 160));
            stageVoice(
              {
                uri,
                durationMs: Math.round(durationMs),
                levels: levels.current.filter((_, index) => index % stride === 0),
              },
              true,
            );
          } else {
            await discard(uri ?? undefined);
            if (mounted.current && focused.current && durationMs < 250)
              void inform(t('media.recordingTooShort'), t('media.recordForAtLeastOneSecondToSend'));
          }
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
        await setAudioModeAsync({ allowsRecording: false });
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
      })
      .catch(() => {});
    return sensitiveCacheWork.track(task);
  }, [recorder, inform, stageVoice, captureOwner, getCaptureLease, setCaptureLease]);
  useEffect(() => {
    if (Platform.OS !== 'ios') return;
    return store.registerDeviceCleanup(async () => {
      const uri = mounted.current ? recorder.uri : undefined;
      await stop();
      // SDK58 releases this SharedObject on unmount and synchronously stops
      // its native recorder. A retained retry must never call that released object.
      if (mounted.current && recorder.getStatus().isRecording) await recorder.stop();
      if (mounted.current && recorder.getStatus().isRecording)
        throw new Error(t('media.theRecordingOnThisDeviceCouldNotBe'));
      discardCacheCopy(uriToDiscard.current);
      if (![...owners.current].some((owner) => owner.uri === uri))
        discardCacheCopy(uri ?? undefined);
    });
  }, [store, stop, recorder]);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
        ++startEpoch.current.value;
        void stop().catch(() => {});
      };
    }, [stop]),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      appState.current = state;
      if (state !== 'active') {
        ++startEpoch.current.value;
        void stop().catch(() => {});
      }
    });
    return () => subscription.remove();
  }, [stop]);
  useLayoutEffect(() => {
    // Search keeps this owner mounted. End a live capture into its retained draft
    // before hiding the recording controls; returning restores the same edits.
    suspendedNow.current = suspended;
    disabledNow.current = disabled;
    if (suspended || disabled) {
      ++startEpoch.current.value;
      void stop().catch(() => {});
    }
  }, [disabled, suspended, stop]);
  useEffect(() => {
    if (recorderState.isRecording && recorderState.durationMillis >= 3_600_000)
      void stop().catch(() => {});
  }, [recorderState.isRecording, recorderState.durationMillis, stop]);

  async function pick(kind: MediaPickKind) {
    try {
      await sensitiveCacheWork.run(() => pickSelection(kind));
    } catch (reason) {
      if (mounted.current) setFailure(failureMessage(reason));
    }
  }
  async function pickSelection(kind: MediaPickKind) {
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
  function start() {
    if (
      busy ||
      startTask.current ||
      stopTask.current ||
      recording.current ||
      clipNow.current ||
      disabledNow.current ||
      suspendedNow.current ||
      !focused.current ||
      appState.current !== 'active' ||
      (channel &&
        (channel.getSnapshot().phase !== 'ready' || channel.getSnapshot().content !== null))
    )
      return;
    const epoch = startEpoch.current.value;
    setBusy(true);
    setFailure(null);
    // Capture recovery before assigning this attempt: its stop must never await
    // the new start that is itself waiting for that old cleanup.
    const recovery = getCaptureLease()?.recover();
    const task = sensitiveCacheWork.run(() =>
      Promise.resolve().then(async () => {
        await recovery;
        if (epoch !== startEpoch.current.value || !mounted.current || getCaptureLease()) return;
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
      });
  }
  async function startRecording(epoch: number) {
    const current = () =>
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
      await setAudioModeAsync({
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
      stoppedCapture.current = null;
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
        await setAudioModeAsync({ allowsRecording: false });
        audioModeNeedsRestore.current = false;
      }
    }
  }
  async function sendEdited(owner: VoiceReviewOwner, captured: VoiceSendable) {
    if (!media || busyNow.current) throw new Error(t('media.voiceRecordingUnavailable'));
    if (clipNow.current?.owner !== owner || hasPendingVoiceTrim(trimNow.current, owner.session))
      throw new Error(t('media.voiceRecordingUnavailable'));
    busyNow.current = true;
    setBusy(true);
    setFailure(null);
    try {
      await stop();
      if (clipNow.current?.owner !== owner || hasPendingVoiceTrim(trimNow.current, owner.session))
        throw new Error(t('media.voiceRecordingUnavailable'));
      const durable = durableOwners.current.get(owner);
      if (durable) await durable.send(captured);
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
      clipNow.current = null;
      setClip(null);
      uriToDiscard.current = undefined;
      void closeOwner(owner).catch(() => {});
      onSent?.();
      void store.refresh().catch(() => {});
    } finally {
      busyNow.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function send() {
    if (!media || busyNow.current || !clip || clip.owner) return;
    busyNow.current = true;
    setBusy(true);
    setFailure(null);
    const uri = clip.uri;
    try {
      await stop();
      await media.sendVoice({
        uri: clip.uri,
        durationMs: clip.durationMs,
        levels: clip.levels,
        spaceId,
        mime: Platform.OS === 'web' ? 'audio/webm' : 'audio/mp4',
        replyTo,
        thread,
      });
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
  async function cancel() {
    if (busyNow.current) return;
    busyNow.current = true;
    setBusy(true);
    try {
      await stop();
      const capture = clipNow.current;
      if (capture?.durable) {
        await capture.durable.discard();
        owners.current.delete(capture.durable.owner);
        durableOwners.current.delete(capture.durable.owner);
      } else if (capture?.owner) await closeOwner(capture.owner);
      else await discard(capture?.uri ?? recorder.uri ?? undefined);
      clipNow.current = null;
      uriToDiscard.current = undefined;
      setClip(null);
      setExpanded(false);
      setFailure(null);
    } catch {
      setFailure(t('media.voiceCleanupFailed'));
    } finally {
      busyNow.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function retryVoice() {
    // Restore the audio session first, without throwing away a successfully staged capture.
    const lease = getCaptureLease();
    if (lease?.owner !== captureOwner) await lease?.recover();
    await stop();
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
    if (!mounted.current || !focused.current) return;
    setFailure(null);
    const owner = clipNow.current?.owner;
    if (!owner) return;
    const current = owner.session.getSnapshot();
    if (current.phase === 'closing' || current.phase === 'closed') await cancel();
    else if (current.phase === 'ready' && current.error && current.error.kind !== 'transcribe')
      await owner.session.retryPreview();
    else if (current.phase === 'idle')
      await (clipNow.current?.durable?.prepare() ?? owner.prepare());
  }
  if (!media) return renderControls?.({ attachment: null, voice: null }) ?? null;
  const isRecording = recorderState.isRecording;
  const voiceActive = Boolean(clip || isRecording);
  const presented = Boolean(attachments.active || clip || isRecording || failure);
  const sheetVisible = Boolean(voiceActive ? expanded : failure);
  const trimPending = Boolean(
    clip?.owner?.session.getSnapshot().pendingTrim ||
    (pendingTrim &&
      clip?.owner?.session === pendingTrim.session &&
      pendingTrim.session.getSnapshot().draft === pendingTrim.draft),
  );
  function collapseVoice() {
    if (
      busy ||
      busyNow.current ||
      (clip?.owner &&
        (hasPendingVoiceTrim(trimNow.current, clip.owner.session) ||
          clip.owner.session.getSnapshot().pendingTrim))
    )
      return;
    pauseAudioPlayback();
    setExpanded(false);
  }
  const review =
    voiceActive && !expanded ? (
      <VoiceDock
        key={clip?.uri ?? 'recording'}
        session={clip?.owner?.session}
        recording={isRecording}
        uri={clip?.uri}
        durationMs={isRecording ? recorderState.durationMillis : (clip?.durationMs ?? 0)}
        levels={isRecording ? recordingLevels : (clip?.levels ?? [])}
        busy={busy}
        failure={failure}
        onCancel={cancel}
        onStop={stop}
        onRetry={failure || clip?.owner ? retryVoice : undefined}
        onExpand={() => {
          pauseAudioPlayback();
          setExpanded(true);
        }}
        onSend={async (value) => {
          if (clip?.owner && value) await sendEdited(clip.owner, value);
          else if (!clip?.owner) await send();
        }}
      />
    ) : null;
  const attachment = (
    <MediaActions
      disabled={
        disabled ||
        busy ||
        attachments.busy ||
        !attachments.canAdd ||
        voiceActive ||
        Boolean(failure)
      }
      hasVoiceEditor={!attachments.active && Boolean(media.voiceEditor)}
      onChoose={(kind) => {
        void pick(kind);
      }}
    />
  );
  const voice = (
    <ComposerAction
      testID="record-voice"
      label={t('media.recordVoiceMessage')}
      name="mic"
      prominent
      disabled={
        disabled ||
        busy ||
        attachments.busy ||
        presented ||
        Boolean(channel && (durableState?.phase !== 'ready' || durableState.content))
      }
      onPress={() => {
        void start();
      }}
    />
  );
  return (
    <>
      {renderControls ? (
        renderControls({
          attachment,
          voice,
          review: disabled ? review : (attachments.review ?? review),
          files: disabled ? undefined : attachments.field,
        })
      ) : (
        <>
          {attachments.review ?? review}
          {attachment}
          {voice}
        </>
      )}
      <MediaSheet
        visible={sheetVisible}
        locked={busy || trimPending}
        onDismiss={() => {
          if (voiceActive) collapseVoice();
          else if (!busyNow.current) void cancel();
        }}
      >
        <ScrollView
          contentInsetAdjustmentBehavior="automatic"
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={{
            flexGrow: 1,
            padding: 24,
            paddingTop: 24,
            paddingBottom: 24,
            gap: 24,
            backgroundColor: 'transparent',
          }}
        >
          <View
            style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}
          >
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 24, fontWeight: '600' }}
            >
              {isRecording
                ? t('media.recording')
                : clip
                  ? t('media.voiceMessage')
                  : t('media.sendAttachment')}
            </Text>
            <IconButton
              label={t(voiceActive ? 'media.voiceCollapse' : 'media.cancelAttachment')}
              name={
                voiceActive
                  ? { ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }
                  : { ios: 'xmark', android: 'close', web: 'close' }
              }
              disabled={busy || trimPending}
              onPress={() => {
                if (voiceActive) collapseVoice();
                else void cancel();
              }}
            />
          </View>
          {isRecording ? (
            <>
              <Text
                accessibilityLiveRegion="polite"
                style={{
                  color: colors.danger,
                  fontSize: 44,
                  textAlign: 'center',
                  fontVariant: ['tabular-nums'],
                }}
              >
                {clipTime(recorderState.durationMillis / 1000)}
              </Text>
              <NativeButton
                label={t('media.stopRecording')}
                onPress={() => {
                  void stop().catch(() => {});
                }}
              />
            </>
          ) : clip?.owner ? (
            <VoiceReview
              key={clip.uri}
              session={clip.owner.session}
              initiallyEditing
              busy={busy}
              onSend={(value) => sendEdited(clip.owner!, value)}
              onPendingTrimChange={onPendingTrimChange}
              onRetryPrepare={() => {
                setFailure(null);
                void (clip.durable?.prepare() ?? clip.owner!.prepare()).catch((reason: unknown) =>
                  setFailure(failureMessage(reason)),
                );
              }}
            />
          ) : clip ? (
            <AudioClip uri={clip.uri} durationMs={clip.durationMs} levels={clip.levels} />
          ) : null}
          {failure ? (
            <View style={{ gap: 12 }}>
              <Text
                accessibilityRole="alert"
                selectable
                style={{ color: colors.danger, lineHeight: 22 }}
              >
                {failure}
              </Text>
              <NativeButton
                secondary
                label={t('media.voiceTryAgain')}
                disabled={busy}
                onPress={() => {
                  void retryVoice().catch((reason: unknown) => {
                    if (mounted.current) setFailure(failureMessage(reason));
                  });
                }}
              />
            </View>
          ) : null}
          {!isRecording && !clip?.owner && Boolean(clip) ? (
            <NativeButton
              label={busy ? t('media.sending') : t('media.send')}
              disabled={busy}
              onPress={() => {
                void send();
              }}
            />
          ) : null}
          <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
            {t('media.reviewBeforeSendingTheFileIsSavedIn')}
          </Text>
        </ScrollView>
      </MediaSheet>
    </>
  );
}
