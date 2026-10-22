import { Stack, router } from 'expo-router';
import { Pressable, View } from 'react-native';
import { t } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { ChatTitle, type ChatHeaderProps } from './chat-title';
import { Icon } from './primitives';

export function ChatHeader(props: ChatHeaderProps) {
  return (
    <Stack.Screen
      options={{
        title: props.title,
        headerShown: true,
        headerBackButtonDisplayMode: 'minimal',
        // Clear inherited transparent appearances so SDK58 keeps UIKit's bar.
        // The system soft edge owns the blur and its transition into content.
        headerTransparent: undefined,
        headerStyle: {},
        headerLargeStyle: {},
        headerBlurEffect: undefined,
        headerBackground: undefined,
        scrollEdgeEffects: { top: 'soft', bottom: 'hidden' },
        headerTitle: () => <ChatTitle {...props} />,
        headerRight: () => <ChatHeaderActions {...props} />,
      }}
    />
  );
}

function ChatHeaderActions(props: ChatHeaderProps) {
  const { presentation: p } = useTheme();
  const search = props.search;
  // UINavigationBar supplies the shared glass background for this item.
  return (
    <View style={{ flexDirection: 'row', width: search ? 88 : 44, height: 44 }}>
      {search ? (
        <Pressable
          testID="chat-search"
          accessibilityRole="button"
          accessibilityLabel={t(search.active ? 'chat.closeSearch' : 'chat.searchThisChat')}
          onPress={search.onToggle}
          style={({ pressed }) => ({
            width: 44,
            height: 44,
            alignItems: 'center',
            justifyContent: 'center',
            opacity: pressed ? 0.65 : 1,
          })}
        >
          <Icon
            name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
            size={22}
            color={p.navigation.control}
          />
        </Pressable>
      ) : null}
      <Pressable
        testID="chat-details-avatar"
        accessibilityRole="button"
        accessibilityLabel={props.title}
        accessibilityHint={t('chat.chatDetails')}
        onPress={() => router.push({ pathname: '/space/[id]', params: { id: props.id } })}
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
    </View>
  );
}
