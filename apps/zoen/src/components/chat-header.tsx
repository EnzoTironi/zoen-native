import { Stack } from 'expo-router';
import { t } from '../i18n/index.ts';
import { ChatTitle, type ChatHeaderProps } from './chat-title';
import { View } from 'react-native';
import { ConversationSearch } from './conversation-search';
import { IconButton } from './primitives';

export function ChatHeader(props: ChatHeaderProps) {
  const search = props.search;
  return (
    <Stack.Screen
      options={{
        title: props.title,
        headerTitle: () =>
          search?.state.active ? <ConversationSearch {...search} /> : <ChatTitle {...props} />,
        headerRight: () =>
          search?.state.active ? (
            <View style={{ flexDirection: 'row' }}>
              <IconButton
                label={t('chat.olderSearchResult')}
                name={{ ios: 'chevron.up', android: 'expand_less', web: 'expand_less' }}
                onPress={() => search.onMove('older')}
              />
              <IconButton
                label={t('chat.newerSearchResult')}
                name={{ ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }}
                onPress={() => search.onMove('newer')}
              />
              <IconButton
                label={t('chat.closeSearch')}
                name={{ ios: 'xmark', android: 'close', web: 'close' }}
                onPress={search.onClose}
              />
            </View>
          ) : (
            <IconButton
              name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
              label={t('chat.searchThisChat')}
              onPress={() => search?.onOpen()}
            />
          ),
      }}
    />
  );
}
