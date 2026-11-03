import * as publicationOperations from './media-composer-publication';
import { resolveVoiceReplyScope } from './media-composer-replies';
import {
  RecordingPresets,
  useAudioRecorder,
  useAudioRecorderState,
  type RecordingStatus,
} from 'expo-audio';
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
import {
  discard,
  failureMessage,
  type Recording,
  type StoppedCapture,
} from './media-composer-shared';
import type { VoiceDraftOwner } from './voice-draft-owner';
import type { VoiceDraft } from './voice-edit';
import type { VoiceReviewOwner } from './voice-review-owner';
import { type VoiceSendable, type VoiceSession } from './voice-session';
import { updatePendingVoiceTrim, type PendingVoiceTrim } from './voice-trim-preview';

import { nativeCaptureOwner, type CaptureLease } from './native-capture-owner.ts';
import { finishNativeCapture, rememberCaptureDuration } from './media-composer-terminal';
import * as captureOperations from './media-composer-capture';
import type { MediaComposerOptions, VoiceCaptureContext } from './media-composer-contract';
import { captureMediaReply } from './media-reply';
import { mediaAcceptedIntent } from './media-acceptance';
import * as lifecycle from './media-composer-lifecycle';
import * as selection from './media-composer-selection';

const subscribeNoDraft = () => () => {};
const noDraft = () => null;
// A departing route retains the audio session until its preparation/restore
// drains. A newly mounted composer must not be interrupted by that old restore.
export function useMediaComposerController({
  spaceId,
  replyTo = null,
  replyQuote = null,
  sharedReply = null,
  replyIntent = null,
  thread = false,
  threadRootId = null,
  disabled = false,
  suspended = false,
  onSent,
  onReplyAccepted,
  onReplyCancelled,
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
  const terminal = useRef<(status: RecordingStatus) => void>(() => {});
  const recorder = useAudioRecorder(
    {
      ...RecordingPresets.HIGH_QUALITY,
      numberOfChannels: 1,
      bitRate: 64000,
      isMeteringEnabled: true,
    },
    (status) => terminal.current(status),
  );
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
  const stoppedCapture = useRef<StoppedCapture | null>(null);
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
  const captureStage = useRef<VoiceCaptureContext | null>(null);
  const [captureReview, setCaptureReview] = useState<VoiceCaptureContext | null>(null);
  const replyContext = useMemo(
    () => captureMediaReply({ replyQuote, sharedReply, replyIntent }),
    [replyQuote, sharedReply, replyIntent],
  );
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
    replyQuote,
    sharedReply,
    replyIntent,
    thread,
    threadRootId,
    disabled: disabled || Boolean(clip) || captureActive || recorderState.isRecording,
    suspended,
    onSent,
    onReplyCancelled,
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
          replyContext,
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
    [media, store, channel, replyTo, replyContext, replaceVoiceControl],
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
  useEffect(() => {
    if (!channel || !store.mediaDrafts.isCurrent(channel)) return;
    const accepted = mediaAcceptedIntent(durableState?.value)?.reply;
    if (accepted) onReplyAccepted?.(accepted);
  }, [channel, store, durableState?.value, onReplyAccepted]);
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

  // Reply changes must not retire the native capture/focus lease. Start owns
  // its reviewed callback; this event-only fallback is for uncaptured legacy audio.
  const stageVoiceNow = useRef(stageVoice);
  useLayoutEffect(() => {
    stageVoiceNow.current = stageVoice;
  }, [stageVoice]);
  const stageCurrentVoice = useCallback(
    (...args: Parameters<typeof stageVoice>) => {
      if (!channel || store.mediaDrafts.isCurrent(channel)) stageVoiceNow.current(...args);
    },
    [channel, store],
  );
  const stop = useCallback(
    () =>
      captureOperations.stopCapture({
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
        stageVoice: stageCurrentVoice,
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
      stageCurrentVoice,
      captureOwner,
      getCaptureLease,
      setCaptureLease,
      replaceVoiceControl,
    ],
  );
  useLayoutEffect(() => {
    rememberCaptureDuration({ recording, stoppedCapture, stopTask }, recorderState.durationMillis);
  }, [recorderState.durationMillis, captureActive]);
  useLayoutEffect(() => {
    terminal.current = (status) =>
      finishNativeCapture(
        {
          recording,
          stoppedCapture,
          stopTask,
          recorder,
          mounted,
          captureControl,
          voiceControlNow,
          getCaptureLease,
          captureOwner,
          setFailure,
        },
        status,
      );
    return () => {
      terminal.current = () => {};
    };
  }, [recorder, stop, getCaptureLease, captureOwner]);
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
    if ((replyQuote || sharedReply) && (!channel || !media?.voiceEditor))
      return Promise.resolve(false);
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
  const publication = {
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
    clip,
    voiceIsActive,
    stop,
    closeOwner,
  };
  function sendEdited(owner: VoiceReviewOwner, captured: VoiceSendable) {
    return publicationOperations.sendEdited(publication, owner, captured);
  }
  function send() {
    return publicationOperations.send(publication);
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
  function cancelVoiceReply() {
    if (
      !controlIsCurrent(voiceControl) ||
      busyNow.current ||
      trimNow.current ||
      disabled ||
      suspended
    )
      return;
    const current = clipNow.current;
    if (!current?.durable || current.durable.channel !== channel) return;
    const previous = current.durable.clearReply();
    if (previous) onReplyCancelled?.(previous);
  }
  return {
    media,
    voiceReplyScope: resolveVoiceReplyScope(
      channel,
      clip,
      captureReview,
      captureActive || recorderState.isRecording,
    ),
    renderControls,
    voiceReplySupported: !(replyQuote || sharedReply) || Boolean(channel && media?.voiceEditor),
    cancelVoiceReply:
      clip?.durable &&
      !durableState?.frozen &&
      !busy &&
      !pendingTrim &&
      (clip.durable.replyContext.shared_reply ||
        clip.durable.replyContext.reply_quote ||
        (clip.durable.replyContext.reply_to && !threadRootId))
        ? cancelVoiceReply
        : undefined,
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
