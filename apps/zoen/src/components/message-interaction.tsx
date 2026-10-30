import {
  createContext,
  use,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentProps,
} from 'react';
import { useFocusEffect } from 'expo-router';
import { AccessibilityInfo, AppState, Pressable, View, useWindowDimensions } from 'react-native';
import {
  GestureDetector,
  GestureStateManager,
  usePanGesture,
  type PanGesture,
} from 'react-native-gesture-handler';
import Animated, {
  Easing,
  useAnimatedStyle,
  useDerivedValue,
  useSharedValue,
  withSequence,
  withTiming,
  type SharedValue,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import * as Haptics from 'expo-haptics';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';
import {
  MESSAGE_EDGE_ZONE,
  messageSwipeAdmission,
  messageSwipeFeedback,
  messageSwipeIntent,
  messageSwipeOffset,
  messageSwipeThreshold,
  type MessageSwipeIntent,
} from '../core/message-swipe.ts';
import { Icon } from './primitives';
import type { MessageInteractionProps } from './message-interaction.types';
import { MessageContextOwnsPress, MessageContextControlBarrier } from './message-context-ports';

const MessagePan = createContext<PanGesture | undefined>(undefined);
const MessageDidPan = createContext<SharedValue<boolean> | undefined>(undefined);
/** Read only from physical press event handlers; SharedValue.get synchronizes with the UI runtime. */
export function useMessagePressGuard() {
  const didPan = use(MessageDidPan);
  return () => !didPan?.get();
}
/** Child RNGH pan/tap calls this on activation to consume the touch before a trailing RN press. */
export function useMessagePressConsumption() {
  const didPan = use(MessageDidPan);
  return () => {
    'worklet';
    didPan?.set(true);
  };
}
/** Keeps nested native media controls independent; accessibility actions bypass physical arbitration. */
export function MessagePressable({
  onPress,
  onLongPress,
  onAccessibilityTap,
  ...props
}: ComponentProps<typeof Pressable>) {
  const canPress = useMessagePressGuard();
  const nativeOwnsPress = use(MessageContextOwnsPress);
  const dismissContext = use(MessageContextControlBarrier);
  function run(callback: () => void, physical = true) {
    if (physical && !canPress()) return;
    if (!dismissContext) callback();
    else
      void dismissContext()
        .then(() => {
          if (!physical || canPress()) callback();
        })
        .catch(() => {});
  }
  return (
    <Pressable
      {...props}
      onPress={
        onPress && !nativeOwnsPress
          ? (event) => {
              if (dismissContext) event.persist();
              run(() => onPress(event));
            }
          : undefined
      }
      onAccessibilityTap={
        onAccessibilityTap ? () => run(() => onAccessibilityTap(), false) : undefined
      }
      onLongPress={
        onLongPress && !nativeOwnsPress
          ? (event) => {
              if (dismissContext) event.persist();
              run(() => onLongPress(event));
            }
          : undefined
      }
    />
  );
}
/** Media scrubbing/tap gestures use `block: useMessagePan()` to win before row actions. */
export function useMessagePan() {
  return use(MessagePan);
}
/** Original native-view arc: two clipped semicircles, driven on the same UI thread as the row. */
function SwipeIndicator({
  direction,
  mine,
  distance,
  success,
  pulse,
  wave,
  exit,
  reducedMotion,
}: {
  direction: MessageSwipeIntent;
  mine: boolean;
  distance: SharedValue<number>;
  success: SharedValue<MessageSwipeIntent | null>;
  pulse: SharedValue<number>;
  wave: SharedValue<number>;
  exit: SharedValue<number>;
  reducedMotion: boolean;
}) {
  const { colors } = useTheme();
  const { scale } = useWindowDimensions();
  const stroke = 2 - 1 / (Number.isFinite(scale) && scale > 0 ? scale : 1);
  const progress = useDerivedValue(() =>
    Math.min(
      1,
      Math.max(0, direction === 'reply' ? -distance.get() : distance.get()) /
        messageSwipeThreshold(mine),
    ),
  );
  const appearance = useAnimatedStyle(() => {
    const value = progress.get(),
      complete = success.get() === direction;
    const foreground = complete ? value : Math.min(1, value * 1.2);
    return {
      opacity: (1 - exit.get()) * (complete ? value : 1),
      transform: [
        {
          scale: reducedMotion
            ? 1
            : (complete ? value : 0.65 + foreground * 0.35) * pulse.get() * (1 - exit.get() * 0.8),
        },
      ],
    };
  });
  const foreground = useAnimatedStyle(() => {
    const value = success.get() === direction ? progress.get() : Math.min(1, progress.get() * 1.2);
    return { opacity: value, transform: [{ scale: reducedMotion ? 1 : value }] };
  });
  const fill = useAnimatedStyle(() => ({ opacity: progress.get() * 0.6 }));
  const successFill = useAnimatedStyle(() => ({
    opacity: success.get() === direction ? progress.get() : 0,
    inset: wave.get() * 9,
    borderRadius: 16.5 - wave.get() * 9,
    borderWidth: stroke + (18 - stroke) * wave.get(),
  }));
  const ring = useAnimatedStyle(() => {
    const amount = success.get() === direction ? wave.get() : 0;
    return { opacity: 1 - amount, transform: [{ scale: 1 + amount * (70 / 33 - 1) }] };
  });
  const rightArc = useAnimatedStyle(() => ({
    transform: [{ rotateZ: `${-180 + Math.min(0.5, progress.get()) * 360}deg` }],
  }));
  const leftArc = useAnimatedStyle(() => ({
    transform: [{ rotateZ: `${-180 + Math.max(0, progress.get() - 0.5) * 360}deg` }],
  }));
  return (
    <Animated.View
      testID={`message-swipe-${direction}`}
      style={[
        {
          position: 'absolute',
          width: 33,
          height: 33,
          top: '50%',
          marginTop: -16.5,
          ...(direction === 'reply'
            ? { right: reducedMotion ? 4 : mine ? -43 : -9 }
            : { left: reducedMotion ? 4 : -43 }),
          borderRadius: 16.5,
          alignItems: 'center',
          justifyContent: 'center',
        },
        appearance,
      ]}
    >
      <Animated.View
        style={[
          { position: 'absolute', inset: 0, borderRadius: 16.5, backgroundColor: colors.muted },
          fill,
        ]}
      />
      <Animated.View
        style={[{ position: 'absolute', inset: 0, borderColor: colors.muted }, successFill]}
      />
      <Animated.View style={[{ position: 'absolute', inset: 0 }, ring]}>
        <View
          style={{ position: 'absolute', left: 16.5, width: 16.5, height: 33, overflow: 'hidden' }}
        >
          <Animated.View
            style={[{ position: 'absolute', left: -16.5, width: 33, height: 33 }, rightArc]}
          >
            <View
              style={{
                position: 'absolute',
                left: 16.5,
                width: 16.5,
                height: 33,
                overflow: 'hidden',
              }}
            >
              <View
                style={{
                  position: 'absolute',
                  left: -16.5,
                  width: 33,
                  height: 33,
                  borderRadius: 16.5,
                  borderWidth: stroke,
                  borderColor: colors.muted,
                }}
              />
            </View>
          </Animated.View>
        </View>
        <View
          style={{ position: 'absolute', left: 0, width: 16.5, height: 33, overflow: 'hidden' }}
        >
          <Animated.View
            style={[{ position: 'absolute', left: 0, width: 33, height: 33 }, leftArc]}
          >
            <View
              style={{ position: 'absolute', left: 0, width: 16.5, height: 33, overflow: 'hidden' }}
            >
              <View
                style={{
                  position: 'absolute',
                  left: 0,
                  width: 33,
                  height: 33,
                  borderRadius: 16.5,
                  borderWidth: stroke,
                  borderColor: colors.muted,
                }}
              />
            </View>
          </Animated.View>
        </View>
      </Animated.View>
      <Animated.View style={foreground}>
        <Icon
          name={
            direction === 'reply'
              ? { ios: 'arrowshape.turn.up.left.fill', android: 'reply', web: 'reply' }
              : { ios: 'bubble.left.and.bubble.right.fill', android: 'forum', web: 'forum' }
          }
          color={colors.text}
          size={17}
        />
      </Animated.View>
    </Animated.View>
  );
}

/** Horizontal-only arbitration leaves taps, native media controls and timeline scrolling intact. */
export function MessageInteraction({
  messageId,
  mine,
  disabled = false,
  swipeEnabled = true,
  accessibilityLabel,
  onActions,
  actionsAccessibilityLabel,
  onQuote,
  onReply,
  onThread,
  children,
  style,
}: MessageInteractionProps) {
  const { reducedMotion } = useTheme();
  const [voiceOver, setVoiceOver] = useState(false);
  useEffect(() => {
    let live = true,
      changed = false;
    const subscription = AccessibilityInfo.addEventListener('screenReaderChanged', (enabled) => {
      changed = true;
      if (live) setVoiceOver(enabled);
    });
    void AccessibilityInfo.isScreenReaderEnabled()
      .then((enabled) => {
        if (live && !changed) setVoiceOver(enabled);
      })
      .catch(() => {});
    return () => {
      live = false;
      subscription.remove();
    };
  }, []);
  const rowBinding = JSON.stringify([mine, messageId, disabled, swipeEnabled]);
  const committedRow = useSharedValue(rowBinding);
  const originX = useSharedValue(0),
    originY = useSharedValue(0);
  const active = useSharedValue(false),
    didPan = useSharedValue(false),
    fired = useSharedValue(false);
  const played = useSharedValue(false),
    success = useSharedValue<MessageSwipeIntent | null>(null);
  const pull = useSharedValue(0),
    distance = useSharedValue(0),
    wave = useSharedValue(0),
    exit = useSharedValue(1),
    pulse = useSharedValue(1),
    epoch = useSharedValue(0),
    observedEpoch = useSharedValue(0),
    activation = useSharedValue(0);
  const lifetime = useRef({
    epoch: 0,
    mounted: false,
    focused: false,
    disabled,
    swipeEnabled,
    quiet: reducedMotion,
    foreground: AppState.currentState === 'active',
    onReply,
    onThread,
  });
  const retire = useCallback(
    (patch: Partial<typeof lifetime.current>) => {
      const current = lifetime.current;
      Object.assign(current, patch);
      current.epoch++;
      epoch.set(current.epoch);
      active.set(false);
      fired.set(false);
      played.set(false);
      success.set(null);
      pull.set(0);
      distance.set(0);
      wave.set(0);
      exit.set(1);
      pulse.set(1);
      // A retired physical touch must not become a trailing RN press when the route resumes.
      didPan.set(true);
    },
    [epoch, active, fired, played, success, pull, distance, wave, exit, pulse, didPan],
  );
  useLayoutEffect(() => {
    committedRow.set(rowBinding);
    retire({ mounted: true, disabled, swipeEnabled });
    return () => retire({ mounted: false });
  }, [retire, committedRow, rowBinding, disabled, swipeEnabled]);
  useLayoutEffect(() => {
    Object.assign(lifetime.current, { quiet: reducedMotion, onReply, onThread });
  }, [reducedMotion, onReply, onThread]);
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) =>
      retire({ foreground: state === 'active' }),
    );
    return () => subscription.remove();
  }, [retire]);
  useFocusEffect(
    useCallback(() => {
      retire({ focused: true });
      return () => retire({ focused: false });
    }, [retire]),
  );
  function admitted(expectedEpoch: number, expectedActivation: number) {
    const current = lifetime.current;
    return (
      current.mounted &&
      current.focused &&
      current.foreground &&
      !current.disabled &&
      current.swipeEnabled &&
      current.epoch === expectedEpoch &&
      activation.get() === expectedActivation
    );
  }
  function tick(expectedEpoch: number, expectedActivation: number) {
    if (
      admitted(expectedEpoch, expectedActivation) &&
      !lifetime.current.quiet &&
      process.env.EXPO_OS === 'ios'
    )
      void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Heavy).catch(() => {});
  }
  function finish(expectedEpoch: number, expectedActivation: number, intent: MessageSwipeIntent) {
    if (admitted(expectedEpoch, expectedActivation) && fired.get())
      (intent === 'reply' ? lifetime.current.onReply : lifetime.current.onThread)?.();
  }
  const pan = usePanGesture({
    enabled: !disabled,
    manualActivation: true,
    cancelsTouchesInView: true,
    onTouchesDown: (event) => {
      'worklet';
      if (committedRow.get() !== rowBinding) {
        GestureStateManager.fail(event.handlerTag);
        return;
      }
      activation.set(activation.get() + 1);
      observedEpoch.set(epoch.get());
      if (event.numberOfTouches === 1) didPan.set(false);
      active.set(false);
      played.set(false);
      success.set(null);
      fired.set(false);
      pull.set(0);
      distance.set(0);
      wave.set(0);
      exit.set(0);
      pulse.set(1);
      const first = event.allTouches[0];
      // Even without an outer swipe, a NEW touch rearms this row's controls.
      // Disabling the recognizer itself would leave retirement's didPan=true.
      if (
        !swipeEnabled ||
        !first ||
        event.numberOfTouches !== 1 ||
        first.absoluteX < MESSAGE_EDGE_ZONE
      ) {
        GestureStateManager.fail(event.handlerTag);
        return;
      }
      originX.set(first.absoluteX);
      originY.set(first.absoluteY);
    },
    onTouchesMove: (event) => {
      'worklet';
      if (
        !swipeEnabled ||
        committedRow.get() !== rowBinding ||
        observedEpoch.get() !== epoch.get() ||
        active.get()
      )
        return;
      const first = event.allTouches[0];
      if (!first || event.numberOfTouches !== 1) {
        GestureStateManager.fail(event.handlerTag);
        return;
      }
      const admission = messageSwipeAdmission(
        originX.get(),
        first.absoluteX - originX.get(),
        first.absoluteY - originY.get(),
        event.numberOfTouches,
      );
      if (admission === 'fail') GestureStateManager.fail(event.handlerTag);
      else if (admission === 'activate') {
        didPan.set(true);
        active.set(true);
        GestureStateManager.activate(event.handlerTag);
      }
    },
    onUpdate: (event) => {
      'worklet';
      if (committedRow.get() !== rowBinding || !active.get() || observedEpoch.get() !== epoch.get())
        return;
      const shown = messageSwipeOffset(event.translationX, mine);
      pull.set(shown);
      distance.set(shown);
      const feedback = messageSwipeFeedback(played.get(), event.translationX, reducedMotion, mine);
      if (feedback.played && !played.get()) {
        success.set(event.translationX < 0 ? 'reply' : 'thread');
        wave.set(reducedMotion ? 1 : withTiming(1, { duration: 300, easing: Easing.linear }));
        if (!reducedMotion)
          pulse.set(
            withSequence(withTiming(1.1, { duration: 200 }), withTiming(1, { duration: 150 })),
          );
      }
      played.set(feedback.played);
      if (feedback.tick) scheduleOnRN(tick, observedEpoch.get(), activation.get());
    },
    onDeactivate: (event) => {
      'worklet';
      if (committedRow.get() !== rowBinding) return;
      const intent =
        !event.canceled && active.get() && !fired.get() && observedEpoch.get() === epoch.get()
          ? messageSwipeIntent(event.translationX, mine)
          : null;
      if (intent) {
        fired.set(true);
        scheduleOnRN(finish, observedEpoch.get(), activation.get(), intent);
      }
    },
    onFinalize: () => {
      'worklet';
      if (committedRow.get() !== rowBinding) return;
      active.set(false);
      exit.set(
        reducedMotion
          ? 1
          : withTiming(1, { duration: 300, easing: Easing.bezier(0.38, 0.7, 0.125, 1.0) }),
      );
      // The source's 0.3s spring token resolves to this CABasic cubic, not a physical spring.
      pull.set(
        reducedMotion
          ? 0
          : withTiming(0, { duration: 300, easing: Easing.bezier(0.38, 0.7, 0.125, 1.0) }),
      );
    },
  });
  const movement = useAnimatedStyle(() => ({
    transform: [{ translateX: reducedMotion ? 0 : pull.get() }],
  }));
  const grouped = voiceOver && Boolean(accessibilityLabel);
  return (
    <MessageDidPan value={didPan}>
      <MessagePan value={pan}>
        <View style={[{ width: '100%' }, style]}>
          <GestureDetector gesture={pan}>
            <Animated.View
              accessible={grouped}
              accessibilityLabel={accessibilityLabel}
              accessibilityRole={
                grouped
                  ? (onActions || onThread || onReply) && !disabled
                    ? 'button'
                    : 'text'
                  : undefined
              }
              accessibilityState={grouped ? { disabled } : undefined}
              accessibilityHint={grouped && !disabled ? actionsAccessibilityLabel : undefined}
              onAccessibilityTap={
                grouped && !disabled ? (onActions ?? onThread ?? onReply) : undefined
              }
              accessibilityActions={
                grouped && !disabled
                  ? [
                      ...(onReply ? [{ name: 'reply', label: t('chat.reply') }] : []),
                      ...(onThread ? [{ name: 'thread', label: t('chat.openReplies') }] : []),
                      ...(onQuote ? [{ name: 'quote', label: t('chat.goToQuotedMessage') }] : []),
                      ...(onActions && actionsAccessibilityLabel
                        ? [{ name: 'actions', label: actionsAccessibilityLabel }]
                        : []),
                    ]
                  : undefined
              }
              onAccessibilityAction={(event) => {
                if (!grouped || disabled) return;
                const name = event.nativeEvent.actionName;
                if (name === 'reply') onReply?.();
                else if (name === 'thread') onThread?.();
                else if (name === 'actions' && actionsAccessibilityLabel) onActions?.();
                else if (name === 'quote') onQuote?.();
              }}
              collapsable={false}
              style={[{ width: '100%', alignItems: mine ? 'flex-end' : 'flex-start' }, movement]}
            >
              <View
                pointerEvents="none"
                accessible={false}
                accessibilityElementsHidden
                style={{ position: 'absolute', inset: 0 }}
              >
                <SwipeIndicator
                  direction="thread"
                  mine={mine}
                  distance={distance}
                  success={success}
                  pulse={pulse}
                  wave={wave}
                  exit={exit}
                  reducedMotion={reducedMotion}
                />
                <SwipeIndicator
                  direction="reply"
                  mine={mine}
                  distance={distance}
                  success={success}
                  pulse={pulse}
                  wave={wave}
                  exit={exit}
                  reducedMotion={reducedMotion}
                />
              </View>
              {children}
            </Animated.View>
          </GestureDetector>
        </View>
      </MessagePan>
    </MessageDidPan>
  );
}
