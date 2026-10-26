import { RecordingPresets, useAudioRecorder, useAudioRecorderState } from 'expo-audio';
import { useFocusEffect } from 'expo-router';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState, Platform } from 'react-native';
import { useDialogs } from '../components/dialogs';
import { useClient } from '../core/provider';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { useAttachmentComposer } from './attachment-composer';
import { pauseAudioPlayback } from './AudioClip';
import { sensitiveCacheWork } from './cache-work';
import type { MediaPickKind } from './media-actions.types';
import { discard, failureMessage, type Recording } from './media-composer-shared';
import type { VoiceDraftOwner } from './voice-draft-owner';
import type { VoiceDraft } from './voice-edit';
import type { VoiceReviewOwner } from './voice-review-owner';
import { type VoiceSendable, type VoiceSession } from './voice-session';
import {
  hasPendingVoiceTrim,
  updatePendingVoiceTrim,
  type PendingVoiceTrim,
} from './voice-trim-preview';

import { nativeCaptureOwner, type CaptureLease } from './native-capture-owner.ts';
import * as captureOperations from './media-composer-capture';
import type { MediaComposerOptions } from './media-composer-contract';
import * as lifecycle from './media-composer-lifecycle';
import * as selection from './media-composer-selection';

const subscribeNoDraft = () => () => {};
const noDraft = () => null;
// A departing route retains the audio session until its preparation/restore
// drains. A newly mounted composer must not be interrupted by that old restore.
export function useMediaComposerController({
  spaceId,
  replyTo = null,
  thread = false,
  threadRootId = null,
  disabled = false,
  suspended = false,
  onSent,
  renderControls,
}: MediaComposerOptions) {
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
    () => (Platform.OS === 'web' ? webCaptureLease.current : nativeCaptureOwner.get()),
    [],
  );
  const setCaptureLease = useCallback((lease: CaptureLease | null) => {
    // Web has independent MediaRecorders and no shared native audio mode. A
    // failed Web preparation must not lock microphones in future route instances.
    if (Platform.OS === 'web') webCaptureLease.current = lease;
    else nativeCaptureOwner.set(lease);
  }, []);
  const [expanded, setExpanded] = useState(false);
  const [clip, setClip] = useState<Recording | null>(null);
  const [busy, setBusy] = useState(false);
  const [captureActive, setCaptureActive] = useState(false);
  const [capturePaused, setCapturePaused] = useState(false);
  const [recordingLocked, setRecordingLocked] = useState(false);
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
  const playbackRetired = useRef(false);
  const focused = useRef(false);
  const selecting = useRef(false);
  const busyNow = useRef(false);
  const clipNow = useRef<Recording | null>(null);
  // Ephemeral UI admission only: a retained control from A must never affect B.
  // A capture keeps its token when Stop materializes its own review draft.
  const [voiceControl, setVoiceControl] = useState<object>(() => ({}));
  const voiceControlNow = useRef(voiceControl);
  const captureControl = useRef<object | null>(null);
  const replaceVoiceControl = useCallback((control: object = {}) => {
    voiceControlNow.current = control;
    setVoiceControl(control);
  }, []);
  const controlIsCurrent = useCallback(
    (control: object) => control === voiceControlNow.current && mounted.current,
    [],
  );
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
    disabled: disabled || Boolean(clip) || captureActive || recorderState.isRecording,
    suspended,
    onSent,
  });
  const closeOwner = useCallback(async (owner: VoiceReviewOwner) => {
    await owner.close();
    owners.current.delete(owner);
    durableOwners.current.delete(owner);
  }, []);
  const stageVoice = useCallback(
    (capture: Omit<Recording, 'owner'>, preserveExpansion = false, control?: object) =>
      captureOperations.stageVoice(
        {
          voiceControlNow,
          replaceVoiceControl,
          channel,
          media,
          replyTo,
          store,
          mounted,
          setFailure,
          owners,
          durableOwners,
          clipNow,
          uriToDiscard,
          setClip,
          setExpanded,
        },
        capture,
        preserveExpansion,
        control,
      ),
    [media, store, channel, replyTo, replaceVoiceControl],
  );
  useEffect(() => lifecycle.voiceLifecycle1({ uriToDiscard, clip }), [clip]);
  useEffect(
    () =>
      lifecycle.voiceLifecycle2({
        mounted,
        owners,
        durableOwners,
        startEpoch,
        closeOwner,
        uriToDiscard,
      }),
    [closeOwner],
  );
  useEffect(() => lifecycle.voiceLifecycle3({ channel, mounted, setFailure }), [channel]);
  const savedVoice = durableState?.content?.kind === 'voice' ? durableState.content : null;
  useEffect(
    () =>
      lifecycle.voiceLifecycle4({
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
      }),
    [channel, savedVoice, media, store, restoreAttempt, replaceVoiceControl],
  );
  useLayoutEffect(
    () =>
      lifecycle.voiceLifecycle5({ clipNow, channel, replaceVoiceControl, setClip, setExpanded }),
    [channel, replaceVoiceControl],
  );
  useEffect(
    () =>
      lifecycle.voiceLifecycle6({
        recorderState: { isRecording: recorderState.isRecording, metering: recorderState.metering },
        levels,
        setRecordingLevels,
      }),
    [recorderState.isRecording, recorderState.durationMillis, recorderState.metering],
  );

  const stop = useCallback(
    () =>
      captureOperations.stopCapture({
        stopTask,
        startTask,
        captureControl,
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
      }),
    [
      recorder,
      inform,
      stageVoice,
      captureOwner,
      getCaptureLease,
      setCaptureLease,
      replaceVoiceControl,
    ],
  );
  useEffect(
    () =>
      lifecycle.voiceLifecycle7({
        store,
        playbackRetired,
        mounted,
        recorder,
        stop,
        uriToDiscard,
        owners,
      }),
    [store, stop, recorder],
  );
  useFocusEffect(
    useCallback(() => lifecycle.voiceLifecycle8({ focused, startEpoch, stop }), [stop]),
  );
  useEffect(() => lifecycle.voiceLifecycle9({ appState, startEpoch, stop }), [stop]);
  useLayoutEffect(
    () =>
      lifecycle.voiceLifecycle10({
        suspendedNow,
        suspended,
        disabledNow,
        disabled,
        startEpoch,
        stop,
      }),
    [disabled, suspended, stop],
  );
  // The same route/lifecycle admission owns preview playback and explicit Send.
  // Hiding controls does not retire callbacks already queued by native UI.
  const voiceIsActive = useCallback(
    () =>
      mounted.current &&
      !playbackRetired.current &&
      focused.current &&
      appState.current === 'active' &&
      !disabledNow.current &&
      !suspendedNow.current,
    [],
  );
  const previewIsCurrent = useCallback(
    (session: VoiceSession) =>
      voiceIsActive() &&
      !busyNow.current &&
      clipNow.current?.owner?.session === session &&
      (!clipNow.current.durable || store.mediaDrafts.isCurrent(clipNow.current.durable.channel)),
    [store, voiceIsActive],
  );
  useEffect(
    () =>
      lifecycle.voiceLifecycle11({
        recorderState: {
          isRecording: recorderState.isRecording,
          durationMillis: recorderState.durationMillis,
        },
        stop,
      }),
    [recorderState.isRecording, recorderState.durationMillis, stop],
  );

  async function pick(kind: MediaPickKind) {
    try {
      await sensitiveCacheWork.run(() => pickSelection(kind));
    } catch (reason) {
      if (mounted.current) setFailure(failureMessage(reason));
    }
  }
  async function pickSelection(kind: MediaPickKind) {
    return selection.pickSelection(
      {
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
      },
      kind,
    );
  }
  function start(held = false): Promise<boolean> {
    return captureOperations.startCapture(
      {
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
      },
      held,
    );
  }
  async function startRecording(epoch: number) {
    return captureOperations.prepareCapture(
      {
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
      },
      epoch,
    );
  }
  async function sendEdited(owner: VoiceReviewOwner, captured: VoiceSendable) {
    if (!media || busyNow.current || !voiceIsActive() || !controlIsCurrent(voiceControl))
      throw new Error(t('media.voiceRecordingUnavailable'));
    if (clipNow.current?.owner !== owner || hasPendingVoiceTrim(trimNow.current, owner.session))
      throw new Error(t('media.voiceRecordingUnavailable'));
    busyNow.current = true;
    pauseAudioPlayback();
    setBusy(true);
    setFailure(null);
    try {
      await stop();
      if (
        !voiceIsActive() ||
        !controlIsCurrent(voiceControl) ||
        clipNow.current?.owner !== owner ||
        hasPendingVoiceTrim(trimNow.current, owner.session)
      )
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
      onSent?.();
      void store.refresh().catch(() => {});
    } finally {
      busyNow.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function send() {
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
  async function cancel() {
    if (!controlIsCurrent(voiceControl) || busyNow.current) return;
    busyNow.current = true;
    pauseAudioPlayback();
    setBusy(true);
    try {
      await stop();
      if (voiceControlNow.current !== voiceControl) return;
      const capture = clipNow.current;
      if (capture?.durable) {
        await capture.durable.discard();
        owners.current.delete(capture.durable.owner);
        durableOwners.current.delete(capture.durable.owner);
      } else if (capture?.owner) await closeOwner(capture.owner);
      else await discard(capture?.uri ?? recorder.uri ?? undefined);
      if (voiceControlNow.current !== voiceControl) return;
      replaceVoiceControl();
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
  async function toggleCapturePause() {
    return captureOperations.toggleCapturePause({
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
    });
  }
  async function retryVoice() {
    return selection.retryVoice({
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
    });
  }
  return {
    media,
    renderControls,
    captureActive,
    recorderState,
    clip,
    attachments,
    failure,
    expanded,
    pendingTrim,
    controlIsCurrent,
    voiceControl,
    busy,
    busyNow,
    trimNow,
    setExpanded,
    capturePaused,
    recordingLocked,
    toggleCapturePause,
    recordingLevels,
    cancel,
    stop,
    retryVoice,
    sendEdited,
    send,
    disabled,
    pick,
    channel,
    durableState,
    start,
    setRecordingLocked,
    setFailure,
    suspended,
    previewIsCurrent,
    colors,
    onPendingTrimChange,
    mounted,
  };
}
