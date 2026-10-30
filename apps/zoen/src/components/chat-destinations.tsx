import { FlashList } from '@shopify/flash-list';
import { Keyboard, Pressable, Text } from 'react-native';
import type { Snapshot } from '../core/types';
import { ConversationAvatar } from '../saved-messages/conversation-avatar';
import { conversationTitle } from '../saved-messages/presentation';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { EmptyState } from './primitives';

export function ChatDestinations({
  chats,
  busy,
  onChoose,
}: {
  chats: Snapshot['conversations'];
  busy: boolean;
  onChoose: (id: string) => void;
}) {
  const { colors } = useTheme();
  return (
    <FlashList
      contentInsetAdjustmentBehavior="never"
      keyboardShouldPersistTaps="handled"
      keyboardDismissMode="interactive"
      data={chats}
      keyExtractor={(chat) => chat.id}
      ListEmptyComponent={<EmptyState title={t('chat.noChatsFound')} />}
      renderItem={({ item }) => (
        <Pressable
          disabled={busy}
          accessibilityRole="button"
          accessibilityLabel={conversationTitle(item)}
          onPress={() => {
            Keyboard.dismiss();
            onChoose(item.id);
          }}
          style={{
            flexDirection: 'row',
            alignItems: 'center',
            gap: 14,
            paddingHorizontal: 24,
            paddingVertical: 16,
            opacity: busy ? 0.5 : 1,
          }}
        >
          <ConversationAvatar chat={item} />
          <Text style={{ color: colors.text, fontSize: 17, flex: 1 }}>
            {conversationTitle(item)}
          </Text>
        </Pressable>
      )}
    />
  );
}
