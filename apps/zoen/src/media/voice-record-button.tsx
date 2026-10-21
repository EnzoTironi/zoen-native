import { useLayoutEffect, useRef, useState } from 'react';
import { View } from 'react-native';
import {
  GestureDetector,
  useExclusiveGestures,
  usePanGesture,
  useTapGesture,
} from 'react-native-gesture-handler';
import Animated, {
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import * as Haptics from 'expo-haptics';
import { Icon } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import {
  recordingLockProgress,
  recordingMove,
  recordingRelease,
  telegramAudio,
} from '../core/telegram-audio-presentation';
import { VoiceHoldIntent } from './voice-hold-intent';
import { ComposerControlSurface, ComposerGlyph } from '../components/composer-control-surface';

export function VoiceRecordButton({
  disabled,
  active,
  locked,
  onStart,
  onReview,
  onCancel,
  onLock,
  onError,
}: {
  disabled: boolean;
  active: boolean;
  locked: boolean;
  onStart: (held: boolean) => Promise<boolean>;
  onReview: () => Promise<void>;
  onCancel: () => Promise<void>;
  onLock: () => void;
  onError: (reason: unknown) => void;
}) {
  const { presentation: p, reducedMotion } = useTheme();
  const native = process.env.EXPO_OS === 'ios';
  const size = native ? 40 : 52;
  const live = useRef({ disabled, onStart, onReview, onCancel, onLock, onError });
  const alive = useRef(true);
  const held = useRef(false);
  const [holding, setHolding] = useState(false);
  const lockProgress = useSharedValue(0);
  const iconScale = useSharedValue(1);
  const [intent] = useState(
    () =>
      new VoiceHoldIntent({
        start: () => onStart(true),
        review: onReview,
        cancel: onCancel,
        locked: onLock,
        error: onError,
      }),
  );
  useLayoutEffect(() => {
    live.current = { disabled, onStart, onReview, onCancel, onLock, onError };
    intent.update({
      start: () => (disabled ? Promise.resolve(false) : onStart(true)),
      review: onReview,
      cancel: onCancel,
      locked: () => {
        onLock();
        void Haptics.selectionAsync().catch(() => {});
      },
      error: onError,
    });
  });
  useLayoutEffect(() => {
    alive.current = true;
    intent.activate();
    return () => {
      alive.current = false;
      intent.retire();
      // A presentation change may replace the mic while the capture owner stays mounted.
      // Drain a held capture into review; the native owner coalesces route cleanup too.
      if (held.current) void live.current.onReview().catch(() => {});
    };
  }, [intent]);
  const wasActive = useRef(active);
  useLayoutEffect(() => {
    if (wasActive.current && !active) intent.completed();
    wasActive.current = active;
  }, [active, intent]);
  useLayoutEffect(() => {
    iconScale.set(
      reducedMotion
        ? 1
        : withTiming(active ? 0.3 : 1, {
            duration: telegramAudio.iconTransitionMs,
            easing: Easing.bezier(...telegramAudio.easeInOut),
          }),
    );
  }, [active, reducedMotion, iconScale]);
  const reset = () => {
    held.current = false;
    if (alive.current) setHolding(false);
    lockProgress.set(0);
  };
  const pan = usePanGesture({
    // A newly busy recorder must not cancel the pointer that admitted that recording.
    enabled: holding || !disabled,
    runOnJS: true,
    minDistance: 0,
    maxPointers: 1,
    activateAfterLongPress: telegramAudio.recordActivationMs,
    onActivate: () => {
      if (!alive.current || live.current.disabled) return;
      held.current = true;
      setHolding(true);
      intent.begin();
    },
    onUpdate: (event) => {
      if (!alive.current || !held.current) return;
      lockProgress.set(recordingLockProgress(event.translationY));
      const decision = recordingMove(event.translationX, event.translationY);
      if (decision) {
        intent.end(decision);
        reset();
      }
    },
    onDeactivate: (event) => {
      if (!alive.current || !held.current) return;
      intent.end(
        event.canceled
          ? 'review'
          : recordingRelease(
              event.translationX,
              event.translationY,
              event.velocityX,
              event.velocityY,
            ),
      );
      reset();
    },
    onFinalize: () => {
      if (held.current) intent.end('review');
      reset();
    },
  });
  function startHandsFree() {
    if (!alive.current || live.current.disabled) return;
    live.current.onLock();
    void live.current.onStart(false).catch(live.current.onError);
  }
  const tap = useTapGesture({
    enabled: !disabled,
    runOnJS: true,
    maxDuration: telegramAudio.recordActivationMs,
    onDeactivate: (event) => {
      if (event.canceled) return;
      // Zoen's existing tap-to-record remains the accessible hands-free alternative.
      startHandsFree();
    },
  });
  const gesture = useExclusiveGestures(pan, tap);
  const icon = useAnimatedStyle(() => ({ transform: [{ scale: iconScale.value }] }));
  const lock = useAnimatedStyle(() => ({
    height: telegramAudio.lockHeight - 32 * lockProgress.value,
    transform: [{ translateY: -16 * lockProgress.value }],
  }));
  return (
    <GestureDetector gesture={gesture}>
      <View
        testID="record-voice"
        collapsable={false}
        accessible
        accessibilityRole="button"
        accessibilityLabel={t('media.recordVoiceMessage')}
        accessibilityHint={t('voiceControls.holdHint')}
        accessibilityState={{ disabled, busy: active }}
        accessibilityActions={[{ name: 'activate' }]}
        onAccessibilityTap={startHandsFree}
        hitSlop={Math.max(0, (44 - size) / 2)}
        onAccessibilityAction={(event) => {
          if (event.nativeEvent.actionName === 'activate') startHandsFree();
        }}
        style={{
          width: size,
          height: size,
          borderRadius: size / 2,
          backgroundColor: native ? 'transparent' : p.chat.input.action,
          alignItems: 'center',
          justifyContent: 'center',
          opacity: disabled && !active ? 0.4 : 1,
        }}
      >
        {holding && !locked ? (
          <Animated.View
            pointerEvents="none"
            style={[
              {
                position: 'absolute',
                bottom: 122,
                width: telegramAudio.lockWidth,
                borderRadius: 20,
                alignItems: 'center',
                justifyContent: 'space-around',
                backgroundColor: p.chat.input.fill,
              },
              lock,
            ]}
          >
            <Icon
              name={{ ios: 'lock.fill', android: 'lock', web: 'lock' }}
              size={18}
              color={p.chat.input.control}
            />
            <Icon
              name={{ ios: 'chevron.up', android: 'expand_less', web: 'expand_less' }}
              size={16}
              color={p.chat.input.control}
            />
          </Animated.View>
        ) : null}
        {native && !active ? (
          <View pointerEvents="none" style={{ position: 'absolute', inset: 0, borderRadius: 20 }}>
            <ComposerControlSurface>
              <View />
            </ComposerControlSurface>
          </View>
        ) : null}
        <Animated.View pointerEvents="none" style={icon}>
          <ComposerGlyph
            name="mic"
            color={native ? p.chat.input.panelControl : p.chat.input.actionText}
          />
        </Animated.View>
      </View>
    </GestureDetector>
  );
}
