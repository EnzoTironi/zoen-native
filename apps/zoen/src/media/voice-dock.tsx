import { useRef, useState, useSyncExternalStore } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { GestureDetector, usePanGesture } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withSpring } from 'react-native-reanimated';
import { IconButton } from '../components/primitives';
import { ComposerAction } from '../components/composer-action';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { AudioClip, clipTime, pauseAudioPlayback } from './AudioClip';
import type { VoiceDockProps } from './voice-dock.types';

const subscribeEmpty = () => () => {};
const emptySnapshot = () => null;

/** A compact view of the same native lease; expanding never creates a second draft. */
export function VoiceDock(props: VoiceDockProps) {
  const { colors, reducedMotion } = useTheme();
  const state = useSyncExternalStore(
    props.session?.subscribe ?? subscribeEmpty,
    props.session?.getSnapshot ?? emptySnapshot,
    props.session?.getSnapshot ?? emptySnapshot,
  );
  const [localFailure, setLocalFailure] = useState<string | null>(null);
  const pending = useRef(false);
  const lift = useSharedValue(0);
  const locked = props.busy;
  // The current preview becomes sendable before rendering is cleared in finally.
  // Match expanded review: keep Send disabled until that preparation cycle settles.
  const captured = state?.rendering ? null : props.session?.getSendable();
  const preview = captured?.preview;
  const uri = props.session ? preview?.uri : props.uri;
  const durationMs = preview?.durationMs ?? props.durationMs;
  const levels = preview?.levels ?? props.levels;
  const tooShort = state?.phase === 'ready' && state.projection && !state.projection.canSend;
  const error =
    props.failure ??
    localFailure ??
    (state?.error
      ? t(
          state.error.kind === 'prepare'
            ? 'media.voicePrepareFailed'
            : state.error.kind === 'transcribe'
              ? 'media.voiceTranscriptionFailed'
              : state.error.kind === 'cleanup'
                ? 'media.voiceCleanupFailed'
                : 'media.voicePreviewFailed',
        )
      : null);
  async function action(work: () => Promise<void>) {
    if (pending.current || locked) return;
    pending.current = true;
    setLocalFailure(null);
    try {
      pauseAudioPlayback();
      await work();
    } catch (reason) {
      setLocalFailure(
        reason instanceof Error ? reason.message : t('media.thisCouldNotBeCompletedTryAgain'),
      );
    } finally {
      pending.current = false;
    }
  }
  function expand() {
    if (locked || pending.current) return;
    pauseAudioPlayback();
    props.onExpand();
  }
  const pan = usePanGesture({
    enabled: !locked,
    runOnJS: true,
    activeOffsetY: -12,
    failOffsetX: [-28, 28],
    onUpdate: (event) => {
      if (!reducedMotion) lift.set(Math.max(-14, Math.min(0, event.translationY * 0.2)));
    },
    onDeactivate: (event) => {
      if (!event.canceled && event.translationY < -32) expand();
    },
    onFinalize: () => {
      lift.set(reducedMotion ? 0 : withSpring(0, { damping: 22, stiffness: 240 }));
    },
  });
  const movement = useAnimatedStyle(() => ({ transform: [{ translateY: lift.value }] }));
  return (
    <GestureDetector gesture={pan}>
      <Animated.View
        testID="voice-dock"
        accessibilityHint={t('media.voiceExpandHint')}
        style={[
          {
            borderRadius: 26,
            paddingHorizontal: 6,
            paddingVertical: 8,
            backgroundColor: colors.surface,
            gap: 4,
          },
          movement,
        ]}
      >
        <View
          style={{
            alignSelf: 'center',
            width: 26,
            height: 3,
            borderRadius: 2,
            backgroundColor: colors.tertiary,
            opacity: 0.5,
          }}
        />
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 2, minHeight: 44 }}>
          <IconButton
            label={t('media.cancelAttachment')}
            name={{ ios: 'trash', android: 'delete', web: 'delete' }}
            disabled={locked}
            onPress={() => {
              void action(props.onCancel);
            }}
          />
          <View style={{ flex: 1, minWidth: 0 }}>
            {props.recording ? (
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
                <View
                  style={{ width: 7, height: 7, borderRadius: 4, backgroundColor: colors.danger }}
                />
                <Text
                  accessibilityLiveRegion="polite"
                  style={{ color: colors.text, fontVariant: ['tabular-nums'] }}
                >
                  {clipTime(props.durationMs / 1000)}
                </Text>
                <View
                  accessible={false}
                  style={{
                    flex: 1,
                    flexDirection: 'row',
                    alignItems: 'center',
                    gap: 2,
                    height: 28,
                  }}
                >
                  {props.levels.slice(-18).map((level, index) => (
                    <View
                      key={index}
                      style={{
                        flex: 1,
                        borderRadius: 2,
                        height: Math.max(2, Math.min(1, level) * 26),
                        backgroundColor: colors.danger,
                      }}
                    />
                  ))}
                </View>
              </View>
            ) : uri ? (
              <AudioClip compact uri={uri} durationMs={durationMs} levels={[...levels]} />
            ) : (
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
                {!error && !tooShort ? <ActivityIndicator color={colors.accent} /> : null}
                <Text numberOfLines={1} style={{ flex: 1, color: colors.secondary }}>
                  {t(
                    tooShort
                      ? 'media.recordingTooShort'
                      : error
                        ? 'media.voicePreviewFailed'
                        : 'media.voicePreparing',
                  )}
                </Text>
              </View>
            )}
          </View>
          {error && props.onRetry ? (
            <IconButton
              label={t('media.voiceTryAgain')}
              name={{ ios: 'arrow.clockwise', android: 'refresh', web: 'refresh' }}
              disabled={locked}
              onPress={() => {
                void action(props.onRetry!);
              }}
            />
          ) : null}
          <IconButton
            label={t('media.voiceExpand')}
            name={{ ios: 'chevron.up', android: 'expand_less', web: 'expand_less' }}
            disabled={locked}
            onPress={expand}
          />
          {props.recording ? (
            <ComposerAction
              prominent
              label={t('media.stopRecording')}
              name="stop.fill"
              disabled={locked}
              onPress={() => {
                void action(props.onStop);
              }}
            />
          ) : (
            <ComposerAction
              prominent
              label={t('media.send')}
              name="arrow.up"
              disabled={locked || (props.session ? !captured : !uri)}
              onPress={() => {
                void action(() => props.onSend(captured ?? undefined));
              }}
            />
          )}
        </View>
        {error ? (
          <Text
            accessibilityRole="alert"
            style={{ color: colors.danger, fontSize: 12, paddingHorizontal: 10 }}
          >
            {error}
          </Text>
        ) : null}
      </Animated.View>
    </GestureDetector>
  );
}
