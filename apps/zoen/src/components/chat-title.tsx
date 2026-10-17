import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import { router } from 'expo-router';
import { Pressable, Text, View, useWindowDimensions } from 'react-native';
import type { Persona } from '../core/generated/Persona';
import { t } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { Icon } from './primitives';

export type ChatHeaderProps = {
  id: string;
  title: string;
  counterpart?: Persona | null;
  activityLabel?: string | null;
};

export function ChatTitle({ id, title, counterpart, activityLabel }: ChatHeaderProps) {
  const { colors, scheme } = useTheme();
  const { width, fontScale } = useWindowDimensions();
  const glass = process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable();
  const content = (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: 7,
        paddingLeft: 5,
        paddingRight: 10,
        minHeight: 44,
        width: Math.min(190, Math.max(88, width - 244)),
      }}
    >
      <ProfileAvatar person={counterpart} name={title} size={34} color={counterpart?.tint_hex} />
      <View style={{ flex: 1, minWidth: 0 }}>
        <Text numberOfLines={1} style={{ color: colors.text, fontSize: 16, fontWeight: '600' }}>
          {title}
        </Text>
        {activityLabel && fontScale <= 1.2 ? (
          <Text numberOfLines={1} style={{ color: colors.secondary, fontSize: 11 }}>
            {activityLabel}
          </Text>
        ) : null}
      </View>
      <Icon
        name={{ ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }}
        size={10}
        color={colors.secondary}
      />
    </View>
  );
  return (
    <Pressable
      testID="chat-details-header"
      accessibilityRole="button"
      accessibilityLabel={[title, activityLabel].filter(Boolean).join(', ')}
      accessibilityHint={t('chat.chatDetails')}
      onPress={() => router.push({ pathname: '/space/[id]', params: { id } })}
      style={({ pressed }) => ({ opacity: pressed ? 0.65 : 1 })}
    >
      {glass ? (
        <GlassView
          isInteractive
          colorScheme={scheme}
          glassEffectStyle="regular"
          style={{ borderRadius: 24 }}
        >
          {content}
        </GlassView>
      ) : (
        <View style={{ borderRadius: 24, backgroundColor: colors.surface }}>{content}</View>
      )}
    </Pressable>
  );
}
