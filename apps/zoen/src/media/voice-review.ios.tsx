import { useMemo, useRef, useState, useSyncExternalStore } from 'react';
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
import { AudioClip, pauseAudioPlayback } from './AudioClip';
import type { VoiceEditCommand } from './voice-edit';
import type { VoiceSession, VoiceSessionFailure } from './voice-session';
import { VoiceWaveform } from './voice-waveform.ios';
import { VoiceWords } from './voice-words.ios';
import { getVoiceSuggestions, voiceSuggestionCommands } from './voice-suggestions.ts';
import type { VoiceReviewProps } from './voice-review.types';

type LocalState = { session: VoiceSession; sending: boolean; error: string | null };

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
}: VoiceReviewProps) {
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const { colors, scheme } = useTheme();
  const [local, setLocal] = useState<LocalState>({ session, sending: false, error: null });
  const [editor, setEditor] = useState({ session, open: initiallyEditing });
  const pendingSend = useRef(new Set<VoiceSession>());
  const sending = local.session === session && local.sending;
  const locked = busy || sending || state.phase !== 'ready';
  const error =
    (local.session === session ? local.error : null) ??
    (state.error ? failureLabel(state.error) : null);
  const editable = editor.session === session && editor.open;
  const sendable = session.getSendable();
  // A previous export is never offered as the preview of a new edit. Unmounting AudioClip
  // also releases its Expo player; we pause synchronously before any committed edit/send.
  const preview = !locked && !state.rendering && sendable ? sendable.preview : null;
  const draft = state.draft;
  const suggestions = useMemo(
    () => (draft ? getVoiceSuggestions(draft, formatLocale) : null),
    [draft],
  );
  function acceptSuggestions(ids: readonly string[]) {
    if (busy || pendingSend.current.has(session)) return;
    const current = session.getSnapshot();
    if (current.phase !== 'ready' || !current.draft) return;
    // IDs include the immutable source and exact original ranges. Revalidate the
    // displayed proposals against the live EDL rather than blindly applying stale cuts.
    const commands = voiceSuggestionCommands(current.draft, formatLocale, ids);
    if (commands.length) edit(commands);
  }

  function edit(commands: VoiceEditCommand | readonly VoiceEditCommand[]) {
    if (busy || pendingSend.current.has(session) || session.getSnapshot().phase !== 'ready') return;
    pauseAudioPlayback();
    setLocal({ session, sending: false, error: null });
    try {
      session.edit(commands);
    } catch {
      setLocal({ session, sending: false, error: t('media.voicePreviewFailed') });
    }
  }
  function history(direction: 'undo' | 'redo') {
    if (busy || pendingSend.current.has(session) || session.getSnapshot().phase !== 'ready') return;
    pauseAudioPlayback();
    setLocal({ session, sending: false, error: null });
    try {
      session[direction]();
    } catch {
      setLocal({ session, sending: false, error: t('media.voicePreviewFailed') });
    }
  }
  async function send() {
    if (busy || pendingSend.current.has(session)) return;
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
      {preview ? (
        <AudioClip
          key={preview.uri}
          uri={preview.uri}
          durationMs={preview.durationMs}
          levels={[...preview.levels]}
        />
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
              disabled={locked || state.rendering}
              onPress={() => {
                if (!busy && !pendingSend.current.has(session)) void session.retryPreview();
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
              disabled={locked}
              onPress={() => {
                if (!locked && !pendingSend.current.has(session)) {
                  pauseAudioPlayback();
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
                      disabledModifier(locked || draft[direction].length === 0),
                    ]}
                    onPress={() => history(direction)}
                  />
                </Host>
              ))}
            </View>
          </View>

          {editable ? <VoiceWaveform draft={draft} disabled={locked} onEdit={edit} /> : null}

          <View style={{ gap: 12 }}>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
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
                  disabled={locked || state.transcribing}
                  onPress={() => {
                    if (!busy && !pendingSend.current.has(session)) {
                      pauseAudioPlayback();
                      setEditor({ session, open: true });
                      void session.transcribe(locale);
                    }
                  }}
                />
              </>
            ) : editable && draft.transcript.words.length ? (
              <VoiceWords draft={draft} disabled={locked} onEdit={edit} />
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
            <View style={{ gap: 12 }}>
              <Text
                accessibilityRole="header"
                style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
              >
                {t('media.voiceSuggestions')}
              </Text>
              <Text style={{ color: colors.secondary }}>{t('media.voiceSuggestionsDetail')}</Text>
              {suggestions.fillers.length > 0 ? (
                <NativeButton
                  secondary
                  label={t('media.voiceRemoveFillers', { count: suggestions.fillers.length })}
                  disabled={locked}
                  onPress={() =>
                    acceptSuggestions(suggestions.fillers.map((suggestion) => suggestion.id))
                  }
                />
              ) : null}
              {suggestions.pauses.length > 0 ? (
                <NativeButton
                  secondary
                  label={t('media.voiceShortenPauses', { count: suggestions.pauses.length })}
                  disabled={locked}
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
        disabled={locked || state.rendering || !sendable}
        onPress={() => {
          void send();
        }}
      />
    </View>
  );
}
