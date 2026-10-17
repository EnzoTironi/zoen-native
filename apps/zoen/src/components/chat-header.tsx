import { Stack, router } from 'expo-router';
import { t } from '../i18n/index.ts';
import { ChatTitle, type ChatHeaderProps } from './chat-title';
import { IconButton } from './primitives';

export function ChatHeader(props: ChatHeaderProps) {
  return (
    <Stack.Screen
      options={{
        title: props.title,
        headerTitle: () => <ChatTitle {...props} />,
        headerRight: () => (
          <IconButton
            name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
            label={t('chat.searchThisChat')}
            onPress={() => router.push({ pathname: '/search', params: { space: props.id } })}
          />
        ),
      }}
    />
  );
}
