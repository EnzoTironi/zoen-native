import { View } from 'react-native';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { Icon } from '../components/primitives';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { contrastingInk } from '../theme/telegram';
import { conversationTitle } from './presentation';

export function ConversationAvatar({ chat, size = 48 }: { chat: SpaceSummary; size?: number }) {
  const { colors } = useTheme();
  if (chat.kind === 'saved')
    return (
      <View
        accessible={false}
        accessibilityElementsHidden
        style={{
          width: size,
          height: size,
          borderRadius: size / 2,
          backgroundColor: colors.accent,
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <Icon
          name={{ ios: 'bookmark.fill', android: 'bookmark', web: 'bookmark' }}
          size={size * 0.48}
          color={contrastingInk(colors.accent)}
        />
      </View>
    );
  return (
    <ProfileAvatar
      person={chat.counterpart}
      name={conversationTitle(chat)}
      color={chat.counterpart?.tint_hex}
      size={size}
      agent={chat.counterpart?.kind === 'Agent'}
    />
  );
}
