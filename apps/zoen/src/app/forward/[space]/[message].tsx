import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { Pressable, Text, TextInput, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { useTheme } from '../../../theme';
import { Avatar, EmptyState } from '../../../components/primitives';

export default function Forward() {
  const { space, message } = useLocalSearchParams<{ space: string; message: string }>();
  const { snapshot, store } = useClient();
  const { colors } = useTheme();
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chats =
    snapshot?.conversations.filter(
      (chat) =>
        chat.id !== space && chat.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
    ) ?? [];
  return (
    <View style={{ flex: 1, backgroundColor: colors.surface }}>
      <Stack.Screen options={{ title: 'Encaminhar para…' }} />
      <TextInput
        accessibilityLabel="Buscar conversa"
        value={query}
        onChangeText={setQuery}
        placeholder="Buscar conversa"
        placeholderTextColor={colors.tertiary}
        style={{
          margin: 20,
          padding: 14,
          borderRadius: 16,
          color: colors.text,
          backgroundColor: colors.muted,
          fontSize: 17,
        }}
      />
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ padding: 20, color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <FlashList
        data={chats}
        keyExtractor={(chat) => chat.id}
        ListEmptyComponent={
          <EmptyState
            title="Nenhuma conversa encontrada"
            detail="Comece outra conversa para encaminhar esta mensagem."
          />
        }
        renderItem={({ item }) => (
          <Pressable
            disabled={busy}
            accessibilityRole="button"
            accessibilityLabel={`Encaminhar para ${item.title}`}
            onPress={() => {
              setBusy(true);
              setError(null);
              void store
                .request({
                  op: 'forward_message',
                  source_space: space,
                  message_id: message,
                  target_space: item.id,
                })
                .then(() => {
                  router.dismissTo({ pathname: '/chat/[id]', params: { id: item.id } });
                })
                .catch((failure: unknown) =>
                  setError(
                    failure instanceof Error ? failure.message : 'Não foi possível encaminhar.',
                  ),
                )
                .finally(() => setBusy(false));
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
            <Avatar name={item.title} color={item.counterpart?.tint_hex} />
            <Text style={{ color: colors.text, fontSize: 17, flex: 1 }}>{item.title}</Text>
          </Pressable>
        )}
      />
    </View>
  );
}
