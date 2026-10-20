import { Stack, router } from 'expo-router';
import { Pressable } from 'react-native';
import { t } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { ChatTitle, type ChatHeaderProps } from './chat-title';

export function ChatHeader(props: ChatHeaderProps) {
  const { colors } = useTheme();
  const search = props.search;
  return (
    <>
      <Stack.Screen options={{ title: props.title, headerBackVisible: true }} />
      <Stack.Title asChild>
        <ChatTitle {...props} />
      </Stack.Title>
      <Stack.Toolbar placement="right">
        {search ? (
          <Stack.Toolbar.Button
            icon="magnifyingglass"
            accessibilityLabel={t(search.active ? 'chat.closeSearch' : 'chat.searchThisChat')}
            tintColor={search.active ? colors.accent : colors.text}
            onPress={search.onToggle}
          />
        ) : null}
        <Stack.Toolbar.View hidesSharedBackground>
          <Pressable
            testID="chat-details-avatar"
            accessibilityRole="button"
            accessibilityLabel={props.title}
            accessibilityHint={t('chat.chatDetails')}
            onPress={() => router.push({ pathname: '/space/[id]', params: { id: props.id } })}
            // f1dd7a2 ChatAvatarNavigationNode:82–83,349:44pt item,
            // with3pt inset on each side; use our existing avatar authority.
            style={({ pressed }) => ({
              width: 44,
              height: 44,
              alignItems: 'center',
              justifyContent: 'center',
              opacity: pressed ? 0.65 : 1,
            })}
          >
            <ProfileAvatar
              person={props.counterpart}
              name={props.title}
              color={props.counterpart?.tint_hex}
              size={38}
            />
          </Pressable>
        </Stack.Toolbar.View>
      </Stack.Toolbar>
    </>
  );
}
