import { createContext, use, useEffect, useState } from 'react';
import { AccessibilityInfo, View } from 'react-native';
import {
  GestureDetector,
  GestureStateManager,
  usePanGesture,
  type PanGesture,
} from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withSpring } from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import * as Haptics from 'expo-haptics';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';
import {
  MESSAGE_EDGE_ZONE,
  messageSwipeAdmission,
  MESSAGE_SWIPE_THRESHOLD,
  messageSwipeIntent,
  messageSwipeOffset,
} from '../core/message-swipe.ts';
import { Icon } from './primitives';
import type { MessageInteractionProps } from './message-interaction.types';

const MessagePan = createContext<PanGesture | undefined>(undefined);
/** Media scrubbing/tap gestures use `block: useMessagePan()` to win before row actions. */
export function useMessagePan() {
  return use(MessagePan);
}
function thresholdHaptic() {
  if (process.env.EXPO_OS === 'ios') void Haptics.selectionAsync().catch(() => {});
}

/** Horizontal-only arbitration leaves taps, long presses and vertical timeline scrolling intact. */
export function MessageInteraction({
  messageId,
  mine,
  disabled = false,
  accessibilityLabel,
  onActions,
  onReply,
  onThread,
  children,
  style,
}: MessageInteractionProps) {
  const { colors, reducedMotion } = useTheme();
  const [voiceOver, setVoiceOver] = useState(false);
  useEffect(() => {
    let live = true;
    void AccessibilityInfo.isScreenReaderEnabled()
      .then((enabled) => {
        if (live) setVoiceOver(enabled);
      })
      .catch(() => {});
    const subscription = AccessibilityInfo.addEventListener('screenReaderChanged', setVoiceOver);
    return () => {
      live = false;
      subscription.remove();
    };
  }, []);
  const originX = useSharedValue(0);
  const originY = useSharedValue(0);
  const active = useSharedValue(false);
  const ticked = useSharedValue(false);
  const fired = useSharedValue(false);
  const pull = useSharedValue(0);
  useEffect(() => {
    pull.set(0);
    active.set(false);
    fired.set(false);
  }, [messageId, disabled, pull, active, fired]);
  function finish(intent: 'reply' | 'thread') {
    if (!disabled) (intent === 'reply' ? onReply : onThread)();
  }
  const pan = usePanGesture({
    enabled: !disabled,
    manualActivation: true,
    onTouchesDown: (event) => {
      'worklet';
      active.set(false);
      ticked.set(false);
      fired.set(false);
      const first = event.allTouches[0];
      if (!first || event.numberOfTouches !== 1 || first.absoluteX < MESSAGE_EDGE_ZONE) {
        GestureStateManager.fail(event.handlerTag);
        return;
      }
      originX.set(first.absoluteX);
      originY.set(first.absoluteY);
    },
    onTouchesMove: (event) => {
      'worklet';
      if (active.get()) return;
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
        active.set(true);
        GestureStateManager.activate(event.handlerTag);
      }
    },
    onUpdate: (event) => {
      'worklet';
      pull.set(messageSwipeOffset(event.translationX));
      if (Math.abs(event.translationX) >= MESSAGE_SWIPE_THRESHOLD && !ticked.get()) {
        ticked.set(true);
        scheduleOnRN(thresholdHaptic);
      }
    },
    onDeactivate: (event) => {
      'worklet';
      const intent =
        !event.canceled && active.get() && !fired.get()
          ? messageSwipeIntent(event.translationX, event.velocityX)
          : null;
      if (intent) {
        fired.set(true);
        scheduleOnRN(finish, intent);
      }
    },
    onFinalize: () => {
      'worklet';
      active.set(false);
      pull.set(reducedMotion ? 0 : withSpring(0, { damping: 22, stiffness: 240 }));
    },
  });
  const movement = useAnimatedStyle(() => ({
    transform: [{ translateX: reducedMotion ? 0 : pull.get() }],
  }));
  const leftIndicator = useAnimatedStyle(() => ({
    opacity: Math.min(1, Math.max(0, pull.get()) / 52),
  }));
  const rightIndicator = useAnimatedStyle(() => ({
    opacity: Math.min(1, Math.max(0, -pull.get()) / 52),
  }));
  return (
    <MessagePan value={pan}>
      <View style={[{ width: '100%' }, style]}>
        <View
          pointerEvents="none"
          accessible={false}
          accessibilityElementsHidden
          style={{
            position: 'absolute',
            left: 4,
            right: 4,
            top: 0,
            bottom: 0,
            flexDirection: 'row',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <Animated.View
            style={[
              {
                width: 34,
                height: 34,
                borderRadius: 17,
                alignItems: 'center',
                justifyContent: 'center',
                backgroundColor: colors.accent,
              },
              leftIndicator,
            ]}
          >
            <Icon
              name={{ ios: 'bubble.left.and.bubble.right.fill', android: 'forum', web: 'forum' }}
              color={colors.onAccent}
              size={17}
            />
          </Animated.View>
          <Animated.View
            style={[
              {
                width: 34,
                height: 34,
                borderRadius: 17,
                alignItems: 'center',
                justifyContent: 'center',
                backgroundColor: colors.accent,
              },
              rightIndicator,
            ]}
          >
            <Icon
              name={{ ios: 'arrowshape.turn.up.left.fill', android: 'reply', web: 'reply' }}
              color={colors.onAccent}
              size={17}
            />
          </Animated.View>
        </View>
        <GestureDetector gesture={pan}>
          <Animated.View
            accessible={voiceOver && Boolean(accessibilityLabel)}
            accessibilityLabel={accessibilityLabel}
            accessibilityActions={
              voiceOver && accessibilityLabel && !disabled
                ? [
                    { name: 'reply', label: t('chat.reply') },
                    { name: 'thread', label: t('chat.openReplies') },
                    ...(onActions
                      ? [{ name: 'actions', label: t('chat.messageActions', { name: '' }) }]
                      : []),
                  ]
                : undefined
            }
            onAccessibilityAction={(event) => {
              if (disabled) return;
              const name = event.nativeEvent.actionName;
              if (name === 'reply') onReply();
              else if (name === 'thread') onThread();
              else if (name === 'actions') onActions?.();
            }}
            collapsable={false}
            style={[{ alignItems: mine ? 'flex-end' : 'flex-start' }, movement]}
          >
            {children}
          </Animated.View>
        </GestureDetector>
      </View>
    </MessagePan>
  );
}
