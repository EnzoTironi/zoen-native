import { Stack, router } from 'expo-router';
import { Pressable, View, useWindowDimensions } from 'react-native';
import { t } from '../i18n/index.ts';
import { useNativeCalls } from '../calls/provider';
import { useClient } from '../core/provider';
import { CallHeaderAction } from '../calls/header-action.ios';
import { ProfileAvatar } from '../profile/profile-avatar';
import { ConversationAvatar } from '../saved-messages/conversation-avatar';
import { useTheme } from '../theme';
import { MessageSelectionHeader } from './message-selection-toolbar';
import { ChatTitle, type ChatHeaderProps } from './chat-title';
import { Icon } from './primitives';
import { NavigationGlass } from './navigation-glass';
import { navigationGlassAppearance } from './navigation-appearance';
import { NavigationContainer } from './navigation-container';

export function ChatHeader(props: ChatHeaderProps) {
  const { width } = useWindowDimensions();
  const calls = useNativeCalls();
  const { snapshot } = useClient();
  const chat = snapshot?.conversations.find((chat) => chat.id === props.id);
  const canCall =
    calls.available &&
    chat?.kind === 'Direct' &&
    chat.privacy === 'EndToEnd' &&
    chat.members.length === 2 &&
    props.counterpart?.kind === 'Person';
  return (
    <Stack.Screen
      options={{
        ...navigationGlassAppearance,
        orientation: 'portrait',
        title: props.selection ? String(props.selection.count) : props.title,
        headerBackVisible: !props.selection,
        unstable_headerLeftItems: props.selection ? () => [] : undefined,
        headerShown: true,
        headerBackButtonDisplayMode: 'minimal',
        headerBackground: undefined,
        headerTitle: () =>
          props.selection ? (
            <NavigationContainer
              scrollViewTag={props.scrollViewTag ?? 0}
              metrics={props.navigationMetrics}
            >
              <MessageSelectionHeader {...props.selection} />
            </NavigationContainer>
          ) : (
            <NavigationContainer
              scrollViewTag={props.scrollViewTag ?? 0}
              metrics={props.navigationMetrics}
            >
              <ChatTitle {...props} availableWidth={Math.max(88, width - (canCall ? 264 : 176))} />
            </NavigationContainer>
          ),
        unstable_headerRightItems: () =>
          props.selection
            ? [{ type: 'button', label: t('chat.cancel'), onPress: props.selection.onCancel }]
            : [
                {
                  type: 'custom',
                  hidesSharedBackground: true,
                  element: <ChatHeaderActions {...props} canCall={canCall} />,
                },
              ],
      }}
    />
  );
}

function ChatHeaderActions(props: ChatHeaderProps & { canCall: boolean }) {
  const { presentation: p } = useTheme();
  const search = props.search;
  const canCall = props.canCall;
  const { snapshot } = useClient();
  const chat = snapshot?.conversations.find((chat) => chat.id === props.id);
  return (
    <NavigationGlass>
      <View
        style={{ flexDirection: 'row', width: (search ? 88 : 44) + (canCall ? 44 : 0), height: 44 }}
      >
        {canCall ? <CallHeaderAction space={props.id} /> : null}
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
          accessibilityHint={t(props.saved ? 'peerInfo.mediaTitle' : 'chat.chatDetails')}
          onPress={() =>
            router.push({
              pathname: props.saved ? '/shared-media/[id]' : '/space/[id]',
              params: { id: props.id },
            })
          }
          style={({ pressed }) => ({
            width: 44,
            height: 44,
            alignItems: 'center',
            justifyContent: 'center',
            opacity: pressed ? 0.65 : 1,
          })}
        >
          {chat?.kind === 'saved' ? (
            <ConversationAvatar chat={chat} size={38} />
          ) : (
            <ProfileAvatar
              person={props.counterpart}
              name={props.title}
              color={props.counterpart?.tint_hex}
              size={38}
            />
          )}
        </Pressable>
      </View>
    </NavigationGlass>
  );
}
