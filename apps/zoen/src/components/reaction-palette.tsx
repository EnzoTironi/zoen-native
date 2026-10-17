import { Pressable, ScrollView, Text, View } from 'react-native';
import Animated, {
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import * as Haptics from 'expo-haptics';
import type { TimelineEntry } from '../core/types';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { IconButton } from './primitives';

export type ReactionPaletteProps = Readonly<{
  reactions: readonly TimelineEntry['reactions'][number][];
  pending?: boolean;
  onReact: (emoji: string | null) => void;
  onMore?: () => void;
}>;
export const QUICK_REACTIONS = ['👍', '❤️', '😂', '😮', '😢', '🙏'] as const;
export const MORE_REACTIONS = [
  '👀',
  '🙌',
  '👏',
  '🎉',
  '🔥',
  '💯',
  '✅',
  '🤔',
  '😍',
  '🥰',
  '😘',
  '😅',
  '😊',
  '😎',
  '🤩',
  '🤗',
  '😴',
  '😬',
  '🤯',
  '😡',
  '😭',
  '😱',
  '🤝',
  '💪',
  '👌',
  '✌️',
  '🫶',
  '🤍',
  '💔',
  '🌟',
] as const;

export function ReactionButton({
  emoji,
  mine,
  pending,
  onReact,
}: {
  emoji: string;
  mine: boolean;
  pending: boolean;
  onReact: (emoji: string | null) => void;
}) {
  const { colors, reducedMotion } = useTheme();
  const scale = useSharedValue(1);
  const press = (down: boolean) =>
    scale.set(
      withTiming(down && !reducedMotion ? 0.96 : 1, {
        duration: 150,
        easing: Easing.bezier(0.23, 1, 0.32, 1),
      }),
    );
  const motion = useAnimatedStyle(() => ({ transform: [{ scale: scale.get() }] }));
  return (
    <Animated.View style={motion}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={t(mine ? 'chat.removeReaction' : 'chat.reactWithEmoji', { emoji })}
        accessibilityState={{ selected: mine, disabled: pending }}
        disabled={pending}
        onPressIn={() => press(true)}
        onPressOut={() => press(false)}
        onPress={() => {
          if (process.env.EXPO_OS === 'ios') void Haptics.selectionAsync().catch(() => {});
          onReact(mine ? null : emoji);
        }}
        style={{
          width: 44,
          height: 44,
          alignItems: 'center',
          justifyContent: 'center',
          borderRadius: 22,
          backgroundColor: mine ? colors.muted : 'transparent',
          borderWidth: mine ? 1 : 0,
          borderColor: colors.accent,
          opacity: pending ? 0.4 : 1,
        }}
      >
        <Text style={{ fontSize: 27, lineHeight: 35 }}>{emoji}</Text>
      </Pressable>
    </Animated.View>
  );
}
export function ReactionPalette({
  reactions,
  pending = false,
  onReact,
  onMore,
}: ReactionPaletteProps) {
  const { colors, scheme } = useTheme();
  const contents = (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      keyboardShouldPersistTaps="handled"
      style={{ height: 56, flexGrow: 0 }}
      contentContainerStyle={{ padding: 6, alignItems: 'center' }}
    >
      {QUICK_REACTIONS.map((emoji) => (
        <ReactionButton
          key={emoji}
          emoji={emoji}
          mine={reactions.some((reaction) => reaction.emoji === emoji && reaction.mine)}
          pending={pending}
          onReact={onReact}
        />
      ))}
      {onMore ? (
        <IconButton
          name={{ ios: 'plus', android: 'add', web: 'add' }}
          label={t('chat.moreReactions')}
          disabled={pending}
          onPress={onMore}
        />
      ) : null}
    </ScrollView>
  );
  return process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable() ? (
    <GlassView
      colorScheme={scheme}
      glassEffectStyle="regular"
      style={{ height: 56, borderRadius: 28, overflow: 'hidden' }}
    >
      {contents}
    </GlassView>
  ) : (
    <View
      style={{ height: 56, borderRadius: 28, backgroundColor: colors.raised, overflow: 'hidden' }}
    >
      {contents}
    </View>
  );
}
