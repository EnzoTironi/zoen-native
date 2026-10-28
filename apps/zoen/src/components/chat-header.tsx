import { Stack } from 'expo-router';
import { t } from '../i18n/index.ts';
import { MessageSelectionHeader, MessageSelectionCancel } from './message-selection-toolbar';
import { ChatTitle, type ChatHeaderProps } from './chat-title';
import { IconButton } from './primitives';

export function ChatHeader(props: ChatHeaderProps) {
  return (
    <Stack.Screen
      options={{
        title: props.selection ? String(props.selection.count) : props.title,
        headerBackVisible: !props.selection,
        headerLeft: props.selection ? () => null : undefined,
        headerTitle: () =>
          props.selection ? (
            <MessageSelectionHeader {...props.selection} />
          ) : (
            <ChatTitle {...props} />
          ),
        headerRight: () =>
          props.selection ? (
            <MessageSelectionCancel onCancel={props.selection.onCancel} />
          ) : (
            <IconButton
              name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
              label={t(props.search?.active ? 'chat.closeSearch' : 'chat.searchThisChat')}
              onPress={() => props.search?.onToggle()}
            />
          ),
      }}
    />
  );
}
