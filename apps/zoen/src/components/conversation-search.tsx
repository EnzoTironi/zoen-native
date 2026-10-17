import { TextInput, View } from 'react-native';
import { conversationQueryFits } from '../core/conversation-search';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { IconButton } from './primitives';
import type { ConversationSearchProps } from './conversation-search.types';

export function ConversationSearch({ state, onChange, onSearch, onAsk }: ConversationSearchProps) {
  const { colors } = useTheme();
  const canAsk =
    state.completed &&
    !state.loading &&
    !state.error &&
    state.hits.length === 0 &&
    !state.cursor &&
    Boolean(state.query.trim()) &&
    conversationQueryFits(state.query.trim());
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', width: 180 }}>
      <IconButton
        label={t(canAsk ? 'chat.askAboutChat' : 'chat.searchThisChat')}
        name={
          canAsk
            ? { ios: 'sparkles', android: 'auto_awesome', web: 'auto_awesome' }
            : { ios: 'magnifyingglass', android: 'search', web: 'search' }
        }
        onPress={canAsk ? onAsk : onSearch}
      />
      <TextInput
        autoFocus
        value={state.query}
        onChangeText={onChange}
        onSubmitEditing={onSearch}
        accessibilityLabel={t('chat.searchThisChat')}
        placeholder={t('chat.searchThisChat')}
        placeholderTextColor={colors.secondary}
        style={{ flex: 1, color: colors.text }}
      />
    </View>
  );
}
