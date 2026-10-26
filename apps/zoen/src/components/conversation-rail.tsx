import { type ReactNode } from 'react';
import { Pressable, View } from 'react-native';
import { useKeyboardState } from 'react-native-keyboard-controller';
import { GestureDetector } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { CONVERSATION_RAIL_GRIP } from '../core/conversation-rail-motion';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { useKeyboardMotion } from './keyboard-motion';
import type { ConversationRailMotion } from './use-conversation-rail';

/** Keep the carousel mounted so keyboard gestures preserve its horizontal position. */
export function ConversationRail({
  children,
  onHeight,
  height,
  motion,
  collapsible,
}: {
  children?: ReactNode;
  onHeight: (height: number) => void;
  height: number;
  motion: ConversationRailMotion;
  collapsible: boolean;
}) {
  const { progress } = useKeyboardMotion();
  const visible = useKeyboardState((state) => state.isVisible);
  const { reducedMotion, colors } = useTheme();
  const { visibleHeight, expansion } = motion;
  const container = useAnimatedStyle(() => {
    const fraction = Math.min(1, Math.max(0, progress.value));
    return {
      height: visibleHeight.get(),
      opacity: reducedMotion ? (fraction > 0 ? 0 : 1) : Math.max(0, 1 - fraction * 1.5),
      transform: [{ translateY: reducedMotion ? 0 : -10 * fraction }],
    };
  });
  const body = useAnimatedStyle(() => ({
    height: height * (collapsible ? expansion.get() : 1),
    opacity: collapsible ? expansion.get() : 1,
    overflow: 'hidden',
  }));
  const content = useAnimatedStyle(() => ({
    transform: [
      { translateY: reducedMotion || !collapsible ? 0 : -height * (1 - expansion.get()) },
    ],
  }));
  return (
    <GestureDetector gesture={motion.gesture}>
      <Animated.View
        testID="conversation-rail"
        pointerEvents={visible ? 'none' : 'box-none'}
        accessibilityElementsHidden={visible}
        importantForAccessibility={visible ? 'no-hide-descendants' : 'auto'}
        style={[{ overflow: 'hidden' }, container]}
      >
        <Animated.View
          testID="conversation-rail-cards"
          style={body}
          pointerEvents={motion.collapsed ? 'none' : 'box-none'}
          accessibilityElementsHidden={motion.collapsed}
          importantForAccessibility={motion.collapsed ? 'no-hide-descendants' : 'auto'}
        >
          <Animated.View style={[{ position: 'absolute', top: 0, left: 0, right: 0 }, content]}>
            <View
              onLayout={({ nativeEvent }) => {
                const measured = nativeEvent.layout.height;
                onHeight(Number.isFinite(measured) ? Math.max(0, measured) : 0);
              }}
            >
              {children}
            </View>
          </Animated.View>
        </Animated.View>
        {collapsible && height > 0 ? (
          <Pressable
            testID="conversation-rail-toggle"
            accessibilityRole="button"
            accessibilityLabel={t(
              motion.collapsed ? 'conversationCards.expand' : 'conversationCards.collapse',
            )}
            accessibilityState={{ expanded: !motion.collapsed, disabled: motion.editing }}
            disabled={motion.editing}
            onPress={motion.toggle}
            style={{
              height: CONVERSATION_RAIL_GRIP,
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            <View
              style={{
                height: 4,
                width: 32,
                borderRadius: 2,
                backgroundColor: colors.secondary,
                opacity: motion.editing ? 0.2 : 0.5,
              }}
            />
          </Pressable>
        ) : null}
      </Animated.View>
    </GestureDetector>
  );
}

/** Reserve exactly the visible card height, without a React update on each keyboard frame. */
export function ConversationRailSpacer({ motion }: { motion: ConversationRailMotion }) {
  const { visibleHeight } = motion;
  const style = useAnimatedStyle(() => ({
    height: visibleHeight.get(),
  }));
  return (
    <Animated.View
      testID="conversation-rail-spacer"
      pointerEvents="none"
      accessible={false}
      style={style}
    />
  );
}
