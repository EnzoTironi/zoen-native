// SPDX-License-Identifier: GPL-2.0-or-later
// Translated from Telegram-iOS f1dd7a2; source locations in telegram-composer-motion.ts.
import { createContext, useContext, useLayoutEffect, type ReactNode } from 'react';
import type { StyleProp, ViewStyle } from 'react-native';
import Animated, {
  Easing,
  LinearTransition,
  ReduceMotion,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
  type AnimatedStyle,
} from 'react-native-reanimated';
import {
  composerMotionTarget,
  telegramComposerMotion as motion,
  type ComposerMotionRole,
} from '../core/telegram-composer-motion';
import { useTheme } from '../theme';

const ActionMotion = createContext<{ active: boolean; imageStyle?: AnimatedStyle<ViewStyle> }>({
  active: true,
});
export const useComposerActionMotion = () => useContext(ActionMotion);

export function composerFrameTransition(reducedMotion: boolean) {
  return LinearTransition.duration(reducedMotion ? 0 : motion.frameDuration)
    .easing(Easing.bezier(...motion.frameCurve))
    .reduceMotion(ReduceMotion.System);
}

/** A retained action: changing visibility never replaces the recording gesture or input. */
export function ComposerMotionSlot({
  kind,
  active,
  visible = active,
  hasSend = false,
  children,
  style,
}: {
  kind: ComposerMotionRole;
  active: boolean;
  visible?: boolean;
  hasSend?: boolean;
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
}) {
  const { reducedMotion } = useTheme();
  const target = composerMotionTarget(kind, active, hasSend, visible);
  const opacity = useSharedValue(target.opacity);
  const scale = useSharedValue(target.scale);
  const x = useSharedValue(target.x);
  const imageX = useSharedValue(0);
  const imageY = useSharedValue(0);
  useLayoutEffect(() => {
    const frame = {
      duration: reducedMotion ? 0 : motion.frameDuration,
      easing: Easing.bezier(...motion.frameCurve),
      reduceMotion: ReduceMotion.System,
    };
    const alpha = {
      duration: reducedMotion
        ? 0
        : kind === 'accessory'
          ? motion.accessoryDuration
          : motion.alphaDuration,
      easing: Easing.bezier(...motion.alphaCurve),
      reduceMotion: ReduceMotion.System,
    };
    // Reanimated retargets withTiming from the current presentation value. Only
    // seed the additive arrow entrance when the previous fade actually finished.
    if (kind === 'send' && active && opacity.get() === 0 && !reducedMotion) {
      imageX.set(motion.sendImageOffset.x);
      imageY.set(motion.sendImageOffset.y);
    }
    opacity.set(withTiming(target.opacity, alpha));
    scale.set(
      withTiming(
        target.scale,
        kind === 'accessory'
          ? { ...alpha, duration: reducedMotion ? 0 : motion.accessoryDuration }
          : frame,
      ),
    );
    x.set(withTiming(target.x, frame));
    imageX.set(withTiming(0, frame));
    imageY.set(withTiming(0, frame));
  }, [
    active,
    kind,
    reducedMotion,
    target.opacity,
    target.scale,
    target.x,
    opacity,
    scale,
    x,
    imageX,
    imageY,
  ]);
  const animatedStyle = useAnimatedStyle(() => ({
    opacity: opacity.get(),
    transform: [{ translateX: x.get() }, { scale: scale.get() }],
  }));
  const imageStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: imageX.get() }, { translateY: imageY.get() }],
  }));
  return (
    <ActionMotion value={{ active, imageStyle }}>
      <Animated.View
        testID={`composer-${kind}-motion`}
        pointerEvents={active ? 'box-none' : 'none'}
        accessibilityElementsHidden={!active}
        importantForAccessibility={active ? 'auto' : 'no-hide-descendants'}
        style={[style, animatedStyle]}
      >
        {children}
      </Animated.View>
    </ActionMotion>
  );
}
