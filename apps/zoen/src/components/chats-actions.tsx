import { router } from 'expo-router';
import { View } from 'react-native';
import { t } from '../i18n/index.ts';
import { IconButton } from './primitives';

export function ChatsActions() {
  return (
    <View style={{ flexDirection: 'row' }}>
      <IconButton
        name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
        label={t('chat.searchAllHistory')}
        onPress={() => router.push('/search')}
      />
      <IconButton
        name={{ ios: 'square.and.pencil', android: 'edit_square', web: 'edit_square' }}
        label={t('chat.newChat')}
        onPress={() => router.push('/new-chat')}
      />
    </View>
  );
}
