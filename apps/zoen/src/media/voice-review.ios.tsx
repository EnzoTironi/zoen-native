import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { Host } from '@expo/ui';
import { Button } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  disabled as disabledModifier,
  frame,
  labelStyle,
} from '@expo/ui/swift-ui/modifiers';
import { NativeButton } from '../components/primitives';
import { formatLocale, locale, t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { AudioClipControls, pauseAudioPlayback } from './AudioClip';
import {
  editedToOriginalFrame,
  type FrameRange,
  type VoiceDraft,
  type VoiceEditCommand,
} from './voice-edit';
import { useVoicePreviewPlayback } from './voice-preview-playback';
import {
  hasPendingVoiceTrim,
  updatePendingVoiceTrim,
  type PendingVoiceTrim,
} from './voice-trim-preview';
import type { VoiceSession, VoiceSessionFailure } from './voice-session';
import { VoiceWaveform } from './voice-waveform.ios';
import { VoiceWords } from './voice-words.ios';
import { getVoiceSuggestions, voiceSuggestionCommands } from './voice-suggestions.ts';
import type { VoiceReviewProps } from './voice-review.types';

type LocalState = { session: VoiceSession; sending: boolean; error: string | null };
const subscribeNoPlayback = () => () => {};
const noPlaybackSnapshot = () => null;

function durationLabel(milliseconds: number): string {
  return `${new Intl.NumberFormat(formatLocale, {
    maximumFractionDigits: 2,
  }).format(milliseconds / 1000)} s`;
}
function failureLabel(error: VoiceSessionFailure): string {
  if (error.kind === 'cleanup') return t('media.voiceCleanupFailed');
  if (error.kind === 'prepare') return t('media.voicePrepareFailed');
  if (error.kind === 'transcribe') return t('media.voiceTranscriptionFailed');
  if (error.kind === 'closed' || error.kind === 'not_ready')
    return t('media.voiceRecordingUnavailable');
  return t('media.voicePreviewFailed');
}

/** iPhone review content. The parent supplies the scrolling sheet and owns the session lease. */
export function VoiceReview({
  session,
  busy = false,
  initiallyEditing = false,
  onSend,
  onRetryPrepare,
  onPendingTrimChange,
}: VoiceReviewProps) {
  const sharedPreview = useVoicePreviewPlayback();
  const playbackState = useSyncExternalStore(
    sharedPreview?.playback.subscribe ?? subscribeNoPlayback,
    sharedPreview?.playback.getSnapshot ?? noPlaybackSnapshot,
    sharedPreview?.playback.getSnapshot ?? noPlaybackSnapshot,
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const { colors, scheme, presentation: p } = useTheme();
  const [local, setLocal] = useState<LocalState>({ session, sending: false, error: null });
  const [editor, setEditor] = useState({ session, open: initiallyEditing });
  const pendingSend = useRef(new Set<VoiceSession>());
  const trimNow = useRef<PendingVoiceTrim>(null);
  const [pendingTrim, setPendingTrim] = useState<PendingVoiceTrim>(null);
  const [trimOwner] = useState(() => ({}));
  const live = useRef<{ session: VoiceSession; busy: boolean } | null>({ session, busy });
  useLayoutEffect(() => {
    live.current = { session, busy };
    return () => {
      live.current = null;
    };
  }, [session, busy]);
  const sending = local.session === session && local.sending;
  const locked = busy || sending || state.phase !== 'ready';
  const editLocked = locked || state.attempted;
  const error =
    (local.session === session ? local.error : null) ??
    (state.error ? failureLabel(state.error) : null);
  const editable = editor.session === session && editor.open;
  const sendable = session.getSendable();
  const draft = state.draft;
  const trimPending =
    !!state.pendingTrim || (pendingTrim?.session === session && pendingTrim.draft === draft);
  // A previous export is never offered as the preview of a new edit. Both views
  // control the parent's player; edits revoke it synchronously before retiring a render.
  const preview = !locked && !trimPending && !state.rendering && sendable ? sendable.preview : null;
  const auditionReady = Boolean(
    preview &&
    draft &&
    sharedPreview?.session === session &&
    sharedPreview.preview === preview &&
    sharedPreview.isCurrent(draft) &&
    playbackState?.loaded,
  );
  const playheadOriginalFrame =
    auditionReady && state.projection && state.source && playbackState
      ? editedToOriginalFrame(
          state.projection,
          Math.round(playbackState.currentSeconds * state.source.sampleRate),
        )
      : null;
  function trimPreviewChanged(observed: VoiceDraft, pending: boolean, range?: FrameRange | null) {
    if (live.current?.session !== session) return;
    if (range !== undefined) session.setPendingTrim(observed, range);
    const next = updatePendingVoiceTrim(trimNow.current, session, observed, pending, trimOwner);
    if (next === trimNow.current) return;
    trimNow.current = next;
    setPendingTrim(next);
    if (pending) pauseAudioPlayback();
    onPendingTrimChange?.(session, observed, pending, trimOwner, range);
  }
  useEffect(
    () => () => {
      if (draft) {
        trimNow.current = updatePendingVoiceTrim(trimNow.current, session, draft, false, trimOwner);
        onPendingTrimChange?.(session, draft, false, trimOwner);
      }
    },
    [session, draft, onPendingTrimChange, trimOwner],
  );
  const suggestions = useMemo(
    () => (draft ? getVoiceSuggestions(draft, formatLocale) : null),
    [draft],
  );
  function pendingTransaction() {
    return !!session.getSnapshot().pendingTrim || hasPendingVoiceTrim(trimNow.current, session);
  }
  function acceptSuggestions(ids: readonly string[]) {
    if (busy || state.attempted || pendingSend.current.has(session) || pendingTransaction()) return;
    const current = session.getSnapshot();
    if (current.phase !== 'ready' || !current.draft) return;
    // IDs include the immutable source and exact original ranges. Revalidate the
    // displayed proposals against the live EDL rather than blindly applying stale cuts.
    const commands = voiceSuggestionCommands(current.draft, formatLocale, ids);
    if (commands.length) edit(commands);
  }

  function edit(commands: VoiceEditCommand | readonly VoiceEditCommand[]) {
    if (
      live.current?.session !== session ||
      live.current.busy ||
      session.getSnapshot().attempted ||
      pendingSend.current.has(session) ||
      session.getSnapshot().phase !== 'ready' ||
      !session.canEdit(commands) ||
      (hasPendingVoiceTrim(trimNow.current, session) && !session.getSnapshot().pendingTrim)
    )
      return;
    const before = session.getSnapshot().draft;
    pauseAudioPlayback();
    setLocal({ session, sending: false, error: null });
    try {
      session.edit(commands);
      if (before && session.getSnapshot().draft !== before) trimPreviewChanged(before, false);
    } catch {
      setLocal({ session, sending: false, error: t('media.voicePreviewFailed') });
    }
  }
  function history(direction: 'undo' | 'redo') {
    if (
      live.current?.session !== session ||
      live.current.busy ||
      session.getSnapshot().attempted ||
      pendingSend.current.has(session) ||
      session.getSnapshot().phase !== 'ready' ||
      pendingTransaction()
    )
      return;
    const before = session.getSnapshot().draft;
    pauseAudioPlayback();
    setLocal({ session, sending: false, error: null });
    try {
      session[direction]();
      if (before && session.getSnapshot().draft !== before) trimPreviewChanged(before, false);
    } catch {
      setLocal({ session, sending: false, error: t('media.voicePreviewFailed') });
    }
  }
  async function send() {
    if (
      live.current?.session !== session ||
      live.current.busy ||
      pendingSend.current.has(session) ||
      hasPendingVoiceTrim(trimNow.current, session)
    )
      return;
    const captured = session.getSendable();
    if (!captured || session.getSnapshot().rendering) return;
    pendingSend.current.add(session);
    pauseAudioPlayback();
    setLocal({ session, sending: true, error: null });
    try {
      await onSend(captured);
    } catch (reason) {
      setLocal((previous) =>
        previous.session === session
          ? {
              ...previous,
              error:
                reason instanceof Error
                  ? reason.message
                  : t('media.thisCouldNotBeCompletedTryAgain'),
            }
          : previous,
      );
    } finally {
      pendingSend.current.delete(session);
      setLocal((previous) =>
        previous.session === session ? { ...previous, sending: false } : previous,
      );
    }
  }

  return (
    <View style={{ gap: 16, width: '100%', minWidth: 0 }}>
      {state.source && state.projection ? (
        <Text selectable style={{ color: colors.secondary, fontSize: 14 }}>
          {t('media.voiceOriginalEdited', {
            original: durationLabel(state.source.durationMs),
            edited: durationLabel(state.projection.durationMs),
          })}
        </Text>
      ) : null}

      {state.phase === 'preparing' || state.rendering ? (
        <View accessibilityRole="progressbar" style={{ flexDirection: 'row', gap: 10 }}>
          <ActivityIndicator color={colors.secondary} />
          <Text style={{ color: colors.secondary }}>{t('media.voicePreparing')}</Text>
        </View>
      ) : null}
      {preview && sharedPreview?.session === session && sharedPreview.preview === preview ? (
        <AudioClipControls playback={sharedPreview.playback} levels={preview.levels} />
      ) : null}
      {state.phase === 'ready' && state.projection && !state.projection.canSend ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t('media.recordingTooShort')}
        </Text>
      ) : null}

      {error ? (
        <View style={{ gap: 12 }}>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {error}
          </Text>
          {state.phase === 'ready' && state.error && state.error.kind !== 'transcribe' ? (
            <NativeButton
              secondary
              label={t('media.voicePreviewRetry')}
              disabled={editLocked || state.rendering}
              onPress={() => {
                if (!busy && !session.getSnapshot().attempted && !pendingSend.current.has(session))
                  void session.retryPreview();
              }}
            />
          ) : state.phase === 'idle' && onRetryPrepare ? (
            <NativeButton
              secondary
              label={t('media.voiceTryAgain')}
              disabled={busy || sending}
              onPress={onRetryPrepare}
            />
          ) : null}
        </View>
      ) : null}

      {draft ? (
        <>
          <View
            style={{
              flexDirection: 'row',
              flexWrap: 'wrap',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 12,
            }}
          >
            <NativeButton
              secondary
              label={t(editable ? 'media.voiceDoneEditing' : 'media.voiceEdit')}
              disabled={editLocked || trimPending}
              onPress={() => {
                if (
                  !editLocked &&
                  !session.getSnapshot().pendingTrim &&
                  !pendingSend.current.has(session) &&
                  !hasPendingVoiceTrim(trimNow.current, session)
                ) {
                  setEditor({ session, open: !editable });
                }
              }}
            />
            <View style={{ flexDirection: 'row', gap: 8 }}>
              {(['undo', 'redo'] as const).map((direction) => (
                <Host
                  key={direction}
                  testID={`voice-${direction}-host`}
                  style={{ width: 44, height: 44 }}
                  colorScheme={scheme}
                  seedColor={colors.accent}
                >
                  <Button
                    testID={`voice-${direction}`}
                    label={t(direction === 'undo' ? 'media.voiceUndo' : 'media.voiceRedo')}
                    systemImage={
                      direction === 'undo' ? 'arrow.uturn.backward' : 'arrow.uturn.forward'
                    }
                    modifiers={[
                      buttonStyle('bordered'),
                      labelStyle('iconOnly'),
                      frame({ width: 44, height: 44 }),
                      disabledModifier(editLocked || trimPending || draft[direction].length === 0),
                    ]}
                    onPress={() => history(direction)}
                  />
                </Host>
              ))}
            </View>
          </View>

          {editable ? (
            <VoiceWaveform
              draft={draft}
              disabled={editLocked}
              onTrimPreviewChange={trimPreviewChanged}
              pendingTrim={state.pendingTrim}
              playheadOriginalFrame={playheadOriginalFrame}
              auditionDisabled={!auditionReady}
              auditionLabels={{
                position: t('media.voiceOriginalPosition'),
                hint: t('media.voiceAuditionHint'),
                playFromSelection: t('media.voicePlayFromSelection'),
                seekBackward: t('media.voiceSeekBackward'),
                seekForward: t('media.voiceSeekForward'),
              }}
              onSeekOriginal={(observed, frame) => {
                if (!locked && !pendingTransaction()) sharedPreview?.seekOriginal(observed, frame);
              }}
              onPlayFromSelection={(observed, range) => {
                if (!locked && !pendingTransaction())
                  sharedPreview?.playFromSelection(observed, range);
              }}
              onEdit={(commands) => {
                if (session.getSnapshot().draft === draft) edit(commands);
              }}
            />
          ) : null}

          <View style={{ gap: 12, backgroundColor: p.list.block, borderRadius: 10, padding: 12 }}>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
            >
              {t('media.voiceTranscript')}
            </Text>
            {state.transcribing ? (
              <Text accessibilityRole="progressbar" style={{ color: colors.secondary }}>
                {t('media.voiceTranscribing')}
              </Text>
            ) : null}
            {!draft.transcript ? (
              <>
                <Text style={{ color: colors.secondary }}>
                  {t('media.voiceTranscriptionDetail')}
                </Text>
                <NativeButton
                  secondary
                  testID="voice-transcribe"
                  label={t('media.voiceTranscribe')}
                  disabled={editLocked || state.transcribing}
                  onPress={() => {
                    if (
                      !busy &&
                      !session.getSnapshot().attempted &&
                      !pendingSend.current.has(session)
                    ) {
                      pauseAudioPlayback();
                      setEditor({ session, open: true });
                      void session.transcribe(locale);
                    }
                  }}
                />
              </>
            ) : editable && draft.transcript.words.length ? (
              <VoiceWords draft={draft} disabled={editLocked || trimPending} onEdit={edit} />
            ) : state.projection?.transcript ? (
              <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 24 }}>
                {state.projection.transcript}
              </Text>
            ) : !draft.transcript.text.trim() ? (
              <Text style={{ color: colors.secondary }}>{t('media.voiceNoSpeech')}</Text>
            ) : null}
            {editable && draft.transcript?.text.trim() && !draft.transcript.words.length ? (
              <Text style={{ color: colors.secondary }}>
                {t('media.voiceWordTimingUnavailable')}
              </Text>
            ) : null}
            {state.transcription === 'unavailable' ? (
              <Text style={{ color: colors.secondary }}>
                {t('media.voiceTranscriptionUnavailable')}
              </Text>
            ) : null}
          </View>

          {suggestions && (suggestions.fillers.length > 0 || suggestions.pauses.length > 0) ? (
            <View style={{ gap: 12, backgroundColor: p.list.block, borderRadius: 10, padding: 12 }}>
              <Text
                accessibilityRole="header"
                style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
              >
                {t('media.voiceSuggestions')}
              </Text>
              <Text style={{ color: colors.secondary }}>{t('media.voiceSuggestionsDetail')}</Text>
              {suggestions.fillers.length > 0 ? (
                <NativeButton
                  secondary
                  label={t('media.voiceRemoveFillers', { count: suggestions.fillers.length })}
                  disabled={editLocked || trimPending}
                  onPress={() =>
                    acceptSuggestions(suggestions.fillers.map((suggestion) => suggestion.id))
                  }
                />
              ) : null}
              {suggestions.pauses.length > 0 ? (
                <NativeButton
                  secondary
                  label={t('media.voiceShortenPauses', { count: suggestions.pauses.length })}
                  disabled={editLocked || trimPending}
                  onPress={() =>
                    acceptSuggestions(suggestions.pauses.map((suggestion) => suggestion.id))
                  }
                />
              ) : null}
            </View>
          ) : null}
        </>
      ) : null}

      <NativeButton
        label={t(busy || sending ? 'media.sending' : 'media.send')}
        disabled={locked || trimPending || state.rendering || !sendable}
        onPress={() => {
          void send();
        }}
      />
    </View>
  );
}
