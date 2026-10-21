// SPDX-License-Identifier: GPL-2.0-or-later
// ChatTextInputActionButtonsNode:234-251 at Telegram-iOS f1dd7a2.
import { useCallback, useLayoutEffect, useRef } from 'react';
import { Pressable } from 'react-native';
import Animated, {
  Easing,
  ReduceMotion,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import { composerSendGeometry } from '../core/telegram-composer-presentation';
import { telegramComposerMotion as motion } from '../core/telegram-composer-motion';
import { useTheme } from '../theme';
import type { ComposerActionProps } from './composer-action';
import { ComposerControlSurface, ComposerGlyph } from './composer-control-surface';
import { useComposerActionMotion } from './composer-action-motion';

/** Telegram's send is a solid inset capsule inside a46pt slot, not a glass button. */
export function ComposerAction({
  name,
  label,
  onPress,
  disabled = false,
  prominent = false,
  testID,
  size = 40,
  minimumHeight = 40,
}: ComposerActionProps) {
  const { presentation: p, reducedMotion } = useTheme();
  const { active, imageStyle } = useComposerActionMotion();
  const live = useRef({ active, disabled, onPress });
  const highlightedScale = useSharedValue(1);
  useLayoutEffect(() => {
    live.current = { active, disabled, onPress };
    if (!active || disabled) highlightedScale.set(1);
  }, [active, disabled, onPress, highlightedScale]);
  const press = useCallback(() => {
    if (live.current.active && !live.current.disabled) live.current.onPress();
  }, []);
  const highlightedStyle = useAnimatedStyle(() => ({
    transform: [{ scale: highlightedScale.get() }],
  }));
  const g = composerSendGeometry(minimumHeight);
  return (
    <Pressable
      testID={testID}
      accessible={active}
      accessibilityElementsHidden={!active}
      importantForAccessibility={active ? 'auto' : 'no-hide-descendants'}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled: disabled || !active }}
      disabled={disabled || !active}
      onPress={press}
      onPressIn={() => {
        if (prominent && live.current.active && !live.current.disabled)
          highlightedScale.set(
            withTiming(motion.sendHighlightedScale, {
              duration: reducedMotion ? 0 : motion.sendHighlightDuration,
              easing: Easing.bezier(...motion.alphaCurve),
              reduceMotion: ReduceMotion.System,
            }),
          );
      }}
      onPressOut={() => {
        if (prominent)
          highlightedScale.set(
            withTiming(1, {
              duration: reducedMotion ? 0 : motion.sendHighlightRecoveryDuration,
              easing: Easing.bezier(...motion.alphaCurve),
              reduceMotion: ReduceMotion.System,
            }),
          );
      }}
      hitSlop={2}
      style={({ pressed }) => ({
        width: prominent ? g.slotWidth : size,
        height: prominent ? g.slotHeight : size,
        alignItems: 'center',
        justifyContent: 'center',
        opacity: disabled ? 0.4 : !prominent && pressed ? 0.65 : 1,
      })}
    >
      {prominent ? (
        <Animated.View
          pointerEvents="none"
          style={[
            {
              width: g.fillWidth,
              height: g.fillHeight,
              borderRadius: g.fillRadius,
              backgroundColor: p.chat.input.action,
              alignItems: 'center',
              justifyContent: 'center',
            },
            highlightedStyle,
          ]}
        >
          <Animated.View style={imageStyle}>
            <ComposerGlyph name={name} color={p.chat.input.actionText} />
          </Animated.View>
        </Animated.View>
      ) : (
        <ComposerControlSurface size={size}>
          <ComposerGlyph name={name} color={p.chat.input.panelControl} />
        </ComposerControlSurface>
      )}
    </Pressable>
  );
}
