import { useState, type ReactNode } from 'react';
import { View } from 'react-native';
import { useKeyboardState } from 'react-native-keyboard-controller';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { useTheme } from '../theme';
import { useKeyboardMotion } from './keyboard-motion';

/** Keep the carousel mounted so keyboard gestures preserve its horizontal position. */
export function ConversationRail({
  children,
  onHeight,
}: {
  children: ReactNode;
  onHeight: (height: number) => void;
}) {
  const [height, setHeight] = useState(0);
  const { progress } = useKeyboardMotion();
  const visible = useKeyboardState((state) => state.isVisible);
  const { reducedMotion } = useTheme();
  const motion = useAnimatedStyle(() => {
    const fraction = Math.min(1, Math.max(0, progress.value));
    return {
      height: height * (1 - fraction),
      opacity: reducedMotion ? (fraction > 0 ? 0 : 1) : Math.max(0, 1 - fraction * 1.5),
      transform: [{ translateY: reducedMotion ? 0 : -10 * fraction }],
    };
  });
  return (
    <Animated.View
      pointerEvents={visible ? 'none' : 'box-none'}
      accessibilityElementsHidden={visible}
      importantForAccessibility={visible ? 'no-hide-descendants' : 'auto'}
      style={[{ overflow: 'hidden' }, motion]}
    >
      <View
        style={{ position: 'absolute', top: 0, left: 0, right: 0 }}
        onLayout={({ nativeEvent }) => {
          const measured = nativeEvent.layout.height;
          setHeight(measured);
          onHeight(measured);
        }}
      >
        {children}
      </View>
    </Animated.View>
  );
}

/** Reserve exactly the visible card height, without a React update on each keyboard frame. */
export function ConversationRailSpacer({ height }: { height: number }) {
  const { progress } = useKeyboardMotion();
  const motion = useAnimatedStyle(() => ({
    height: height * (1 - Math.min(1, Math.max(0, progress.value))),
  }));
  return <Animated.View pointerEvents="none" accessible={false} style={motion} />;
}
