import { t } from '../i18n/index.ts';
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { AppState, Platform, ScrollView, Text, View } from 'react-native';
import { useFocusEffect } from 'expo-router';
import * as DocumentPicker from 'expo-document-picker';
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
import type { VoiceSendable } from './voice-session';
import { VoiceReview } from './voice-review';
import { VoiceDock } from './voice-dock';
import { MediaSheet } from './media-sheet';
import { MediaActions } from './media-actions';
import type { MediaPickKind } from './media-actions.types';
import { useAttachmentComposer, type AttachmentComposerField } from './attachment-composer';

type Recording = { uri: string; durationMs: number; levels: number[]; owner?: VoiceReviewOwner };
const failureMessage = (reason: unknown) =>
  reason instanceof Error ? reason.message : t('media.thisCouldNotBeCompletedTryAgain');
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
  disabled = false,
  suspended = false,
  onSent,
  renderControls,
}: {
  spaceId: string;
  replyTo?: string | null;
  thread?: boolean;
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
  const { store } = useClient();
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
  const [expanded, setExpanded] = useState(false);
  const [clip, setClip] = useState<Recording | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const levels = useRef<number[]>([]);
  const [recordingLevels, setRecordingLevels] = useState<readonly number[]>([]);
  const stopTask = useRef<Promise<void> | null>(null);
  const recording = useRef(false);
  const suspendedNow = useRef(suspended);
  const uriToDiscard = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  const focused = useRef(false);
  const selecting = useRef(false);
  const busyNow = useRef(false);
  const clipNow = useRef<Recording | null>(null);
  const owners = useRef(new Set<VoiceReviewOwner>());
  const attachments = useAttachmentComposer({
    spaceId,
    replyTo,
    thread,
    disabled: disabled || Boolean(clip) || recorderState.isRecording,
    suspended,
    onSent,
  });
  const closeOwner = useCallback(async (owner: VoiceReviewOwner) => {
    await owner.close();
    owners.current.delete(owner);
  }, []);
  const stageVoice = useCallback(
    (capture: Omit<Recording, 'owner'>, preserveExpansion = false) => {
      const owner = media?.voiceEditor
        ? new VoiceReviewOwner({
            port: media.voiceEditor,
            uri: capture.uri,
            registerCleanup: (cleanup) => store.registerDeviceCleanup(cleanup),
            work: sensitiveCacheWork,
            discard: discardCacheCopy,
          })
        : undefined;
      if (owner) owners.current.add(owner);
      const value = { ...capture, owner };
      clipNow.current = value;
      uriToDiscard.current = owner ? undefined : capture.uri;
      setClip(value);
      if (!preserveExpansion) setExpanded(false);
      if (owner)
        void owner.prepare().catch((reason: unknown) => {
          if (mounted.current) setFailure(failureMessage(reason));
        });
    },
    [media, store],
  );
  useEffect(() => {
    uriToDiscard.current = clip?.owner ? undefined : clip?.uri;
  }, [clip]);
  useEffect(() => {
    mounted.current = true;
    const retainedOwners = owners.current;
    return () => {
      mounted.current = false;
      for (const owner of retainedOwners) void closeOwner(owner).catch(() => {});
      void discard(uriToDiscard.current);
    };
  }, [closeOwner]);
  useEffect(() => {
    if (!recorderState.isRecording) return;
    if (typeof recorderState.metering === 'number') {
      levels.current.push(Math.min(1, Math.max(0.02, 10 ** (recorderState.metering / 40))));
      setRecordingLevels(levels.current.slice(-18));
    }
  }, [recorderState.isRecording, recorderState.durationMillis, recorderState.metering]);

  const stop = useCallback(() => {
    if (stopTask.current) return stopTask.current;
    if (!recording.current) return Promise.resolve();
    recording.current = false;
    const task = (async () => {
      try {
        const durationMs = recorder.getStatus().durationMillis;
        await recorder.stop();
        const uri = recorder.uri;
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
      } catch (reason) {
        if (mounted.current) setFailure(failureMessage(reason));
      } finally {
        await setAudioModeAsync({ allowsRecording: false });
        stopTask.current = null;
      }
    })();
    stopTask.current = task;
    return sensitiveCacheWork.track(task);
  }, [recorder, inform, stageVoice]);
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
        void stop();
      };
    }, [stop]),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state !== 'active') void stop();
    });
    return () => subscription.remove();
  }, [stop]);
  useLayoutEffect(() => {
    // Search keeps this owner mounted. End a live capture into its retained draft
    // before hiding the recording controls; returning restores the same edits.
    suspendedNow.current = suspended;
    if (suspended) void stop();
  }, [suspended, stop]);
  useEffect(() => {
    if (recorderState.isRecording && recorderState.durationMillis >= 3_600_000) void stop();
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
  async function start() {
    try {
      await sensitiveCacheWork.run(startRecording);
    } catch (reason) {
      if (mounted.current) setFailure(failureMessage(reason));
    }
  }
  async function startRecording() {
    if (
      !media ||
      busy ||
      attachments.active ||
      attachments.busy ||
      disabled ||
      suspendedNow.current ||
      recording.current
    )
      return;
    setBusy(true);
    setFailure(null);
    try {
      const permission = await AudioModule.requestRecordingPermissionsAsync();
      if (!mounted.current || !focused.current || suspendedNow.current) return;
      if (!permission.granted) {
        await inform(
          t('media.allowMicrophone'),
          t('media.allowTheMicrophoneInDeviceSettingsToRecord'),
        );
        return;
      }
      pauseAudioPlayback();
      await setAudioModeAsync({
        playsInSilentMode: true,
        allowsRecording: true,
        shouldPlayInBackground: false,
        allowsBackgroundRecording: false,
        interruptionMode: 'doNotMix',
      });
      if (!mounted.current || !focused.current || suspendedNow.current) {
        await setAudioModeAsync({ allowsRecording: false });
        return;
      }
      await recorder.prepareToRecordAsync();
      if (!mounted.current || !focused.current || suspendedNow.current) {
        await recorder.stop();
        await discard(recorder.uri ?? undefined);
        await setAudioModeAsync({ allowsRecording: false });
        return;
      }
      levels.current = [];
      setRecordingLevels([]);
      setExpanded(false);
      recorder.record();
      recording.current = true;
    } catch (reason) {
      await inform(t('media.couldNotRecord'), failureMessage(reason));
      await setAudioModeAsync({ allowsRecording: false });
    } finally {
      setBusy(false);
    }
  }
  async function sendEdited(owner: VoiceReviewOwner, captured: VoiceSendable) {
    if (!media || busyNow.current) throw new Error(t('media.voiceRecordingUnavailable'));
    busyNow.current = true;
    setBusy(true);
    setFailure(null);
    try {
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
      if (capture?.owner) await closeOwner(capture.owner);
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
  if (!media) return renderControls?.({ attachment: null, voice: null }) ?? null;
  const isRecording = recorderState.isRecording;
  const voiceActive = Boolean(clip || isRecording);
  const presented = Boolean(attachments.active || clip || isRecording || failure);
  const sheetVisible = Boolean(voiceActive ? expanded : failure);
  function collapseVoice() {
    if (busy || busyNow.current) return;
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
        onRetry={
          clip?.owner
            ? async () => {
                setFailure(null);
                const session = clip.owner!.session;
                const phase = session.getSnapshot().phase;
                if (phase === 'closing' || phase === 'closed') await cancel();
                else if (phase === 'ready') await session.retryPreview();
                else await clip.owner!.prepare();
              }
            : undefined
        }
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
      disabled={disabled || busy || attachments.busy || presented}
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
        locked={busy}
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
              disabled={busy}
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
                  void stop();
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
              onRetryPrepare={() => {
                setFailure(null);
                void clip
                  .owner!.prepare()
                  .catch((reason: unknown) => setFailure(failureMessage(reason)));
              }}
            />
          ) : clip ? (
            <AudioClip uri={clip.uri} durationMs={clip.durationMs} levels={clip.levels} />
          ) : null}
          {failure ? (
            <Text
              accessibilityRole="alert"
              selectable
              style={{ color: colors.danger, lineHeight: 22 }}
            >
              {failure}
            </Text>
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
