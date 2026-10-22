import { ScrollView, Text, View } from 'react-native';
import { IconButton, NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { AudioClip, clipTime } from './AudioClip';
import { MediaActions } from './media-actions';
import { failureMessage } from './media-composer-shared';
import { MediaSheet } from './media-sheet';
import { VoiceDock } from './voice-dock';
import { VoicePreviewPlayback } from './voice-preview-playback';
import { VoiceRecordButton } from './voice-record-button';
import { VoiceReview } from './voice-review';
import { hasPendingVoiceTrim } from './voice-trim-preview';

import type { useMediaComposerController } from './use-media-composer-controller';
export function renderMediaComposer(context: ReturnType<typeof useMediaComposerController>) {
  const {
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
  } = context;
  if (!media) return renderControls?.({ attachment: null, voice: null }) ?? null;
  const isRecording = captureActive || recorderState.isRecording;
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
      !controlIsCurrent(voiceControl) ||
      busy ||
      busyNow.current ||
      (clip?.owner &&
        (hasPendingVoiceTrim(trimNow.current, clip.owner.session) ||
          clip.owner.session.getSnapshot().pendingTrim))
    )
      return;
    setExpanded(false);
  }
  const review =
    voiceActive && !expanded ? (
      <VoiceDock
        key={clip?.uri ?? 'recording'}
        session={clip?.owner?.session}
        recording={isRecording}
        paused={capturePaused}
        locked={recordingLocked}
        onPause={toggleCapturePause}
        uri={clip?.uri}
        durationMs={isRecording ? recorderState.durationMillis : (clip?.durationMs ?? 0)}
        levels={isRecording ? recordingLevels : (clip?.levels ?? [])}
        busy={busy}
        failure={failure}
        onCancel={cancel}
        onStop={() => (controlIsCurrent(voiceControl) ? stop() : Promise.resolve())}
        onRetry={failure || clip?.owner ? retryVoice : undefined}
        onExpand={() => {
          if (controlIsCurrent(voiceControl)) setExpanded(true);
        }}
        onSend={async (value) => {
          if (!controlIsCurrent(voiceControl)) return;
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
      onChoose={(kind) => pick(kind)}
    />
  );
  const voice = (
    <VoiceRecordButton
      disabled={
        disabled ||
        busy ||
        attachments.busy ||
        presented ||
        Boolean(channel && (durableState?.phase !== 'ready' || durableState.content))
      }
      active={isRecording}
      locked={recordingLocked}
      onStart={start}
      onReview={async () => {
        if (!controlIsCurrent(voiceControl)) return;
        setRecordingLocked(true);
        await stop();
      }}
      onCancel={async () => {
        if (!controlIsCurrent(voiceControl)) return;
        setRecordingLocked(true);
        await cancel();
      }}
      onLock={() => {
        if (controlIsCurrent(voiceControl)) setRecordingLocked(true);
      }}
      onError={(reason) => {
        if (controlIsCurrent(voiceControl)) setFailure(failureMessage(reason));
      }}
    />
  );
  return (
    <VoicePreviewPlayback
      session={clip?.owner?.session ?? null}
      ownerKey={clip?.uri}
      disabled={busy || disabled || suspended}
      isCurrent={previewIsCurrent}
    >
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
            padding: 16,
            paddingTop: 16,
            paddingBottom: 16,
            gap: 16,
            backgroundColor: 'transparent',
          }}
        >
          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'space-between',
              alignItems: 'center',
              minHeight: 44,
            }}
          >
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
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
                secondary
                label={t(capturePaused ? 'voiceControls.resume' : 'voiceControls.pause')}
                onPress={() => {
                  void toggleCapturePause();
                }}
              />
              <NativeButton
                label={t('media.stopRecording')}
                onPress={() => {
                  if (controlIsCurrent(voiceControl)) void stop().catch(() => {});
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
    </VoicePreviewPlayback>
  );
}
