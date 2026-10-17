import { ProfileAvatar } from '../../../profile/profile-avatar';
import { Stack, router } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { useMemo, useState } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { useTheme } from '../../../theme';
import { EmptyState, IconButton } from '../../../components/primitives';

const filters = [
  { id: 'all', label: 'Todas' },
  { id: 'unread', label: 'Não lidas' },
  { id: 'Direct', label: 'Pessoas' },
  { id: 'Group', label: 'Grupos' },
  { id: 'Community', label: 'Comunidades' },
  { id: 'archived', label: 'Arquivadas' },
] as const;
export default function Chats() {
  const { snapshot, mode, store } = useClient();
  const { colors } = useTheme();
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<string>('all');
  const conversations = useMemo(
    () =>
      (snapshot?.conversations ?? [])
        .filter(
          (chat) =>
            (filter === 'archived' ? chat.archived : !chat.archived) &&
            (filter === 'all' ||
              filter === 'archived' ||
              (filter === 'unread' ? chat.unread > 0 : chat.kind === filter)) &&
            `${chat.title} ${chat.last_preview}`
              .toLocaleLowerCase()
              .includes(query.toLocaleLowerCase()),
        )
        .sort(
          (a, b) =>
            Number(b.pinned) - Number(a.pinned) || Number(b.last_at_ms) - Number(a.last_at_ms),
        ),
    [snapshot, query, filter],
  );
  return (
    <View style={{ flex: 1, backgroundColor: colors.surface }}>
      <Stack.Screen
        options={{
          title: 'Chats',
          headerLargeTitle: true,
          headerSearchBarOptions: {
            placeholder: 'Buscar nas conversas',
            onChangeText: (event) => setQuery(event.nativeEvent.text),
            onSearchButtonPress: () => router.push({ pathname: '/search', params: { query } }),
          },
          headerRight: () => (
            <View style={{ flexDirection: 'row' }}>
              <IconButton
                name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
                label="Buscar em todo o histórico"
                onPress={() => router.push('/search')}
              />
              <IconButton
                name={{ ios: 'square.and.pencil', android: 'edit_square', web: 'edit_square' }}
                label="Nova conversa"
                onPress={() => router.push('/new-chat')}
              />
            </View>
          ),
        }}
      />
      <FlashList
        data={conversations}
        keyExtractor={(chat) => chat.id}
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="on-drag"
        ListHeaderComponent={
          <View style={{ paddingBottom: 12 }}>
            {mode === 'demo' ? (
              <View
                style={{
                  paddingHorizontal: 22,
                  paddingVertical: 10,
                  backgroundColor: colors.muted,
                }}
              >
                <Text style={{ color: colors.secondary, fontSize: 13 }}>
                  Demonstração · conversas simuladas neste aparelho
                </Text>
              </View>
            ) : null}
            <ScrollView
              horizontal
              showsHorizontalScrollIndicator={false}
              contentContainerStyle={{ gap: 8, paddingHorizontal: 20, paddingTop: 12 }}
            >
              {filters.map((item) => (
                <Pressable
                  key={item.id}
                  accessibilityRole="button"
                  accessibilityState={{ selected: filter === item.id }}
                  aria-pressed={process.env.EXPO_OS === 'web' ? filter === item.id : undefined}
                  onPress={() => setFilter(item.id)}
                  style={{
                    minHeight: 36,
                    paddingHorizontal: 14,
                    justifyContent: 'center',
                    borderRadius: 18,
                    backgroundColor: filter === item.id ? colors.text : colors.muted,
                  }}
                >
                  <Text
                    style={{
                      color: filter === item.id ? colors.surface : colors.secondary,
                      fontSize: 14,
                      fontWeight: '600',
                    }}
                  >
                    {item.label}
                  </Text>
                </Pressable>
              ))}
            </ScrollView>
          </View>
        }
        ListEmptyComponent={
          <EmptyState
            title={query ? 'Nada por aqui ainda' : 'Comece uma conversa'}
            detail={
              query
                ? 'Tente outro nome ou palavra.'
                : 'Converse com alguém, reúna um grupo ou crie algo com seu agente.'
            }
          />
        }
        renderItem={({ item: chat }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`${chat.title}, ${chat.unread ? `${chat.unread} não lidas. ` : ''}${chat.last_preview}`}
            onPress={() => router.push({ pathname: '/chat/[id]', params: { id: chat.id } })}
            style={({ pressed }) => ({
              flexDirection: 'row',
              gap: 14,
              paddingHorizontal: 20,
              paddingVertical: 14,
              minHeight: 88,
              alignItems: 'center',
              backgroundColor: pressed ? colors.muted : colors.surface,
            })}
          >
            <ProfileAvatar
              person={chat.counterpart}
              name={chat.title}
              color={chat.counterpart?.tint_hex}
              size={54}
              agent={chat.counterpart?.kind === 'Agent'}
            />
            <View style={{ flex: 1, gap: 6 }}>
              <View
                style={{
                  flexDirection: 'row',
                  justifyContent: 'space-between',
                  alignItems: 'baseline',
                  gap: 8,
                }}
              >
                <Text
                  numberOfLines={1}
                  style={{
                    flex: 1,
                    fontSize: 17,
                    fontWeight: chat.unread ? '700' : '600',
                    color: colors.text,
                  }}
                >
                  {chat.title}
                </Text>
                <Text
                  style={{ fontSize: 12, color: chat.unread ? colors.accent : colors.secondary }}
                >
                  {Number(chat.last_at_ms)
                    ? new Date(Number(chat.last_at_ms)).toLocaleTimeString([], {
                        hour: '2-digit',
                        minute: '2-digit',
                      })
                    : ''}
                </Text>
              </View>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
                <Text
                  numberOfLines={2}
                  style={{ flex: 1, lineHeight: 20, fontSize: 14, color: colors.secondary }}
                >
                  {chat.last_preview}
                </Text>
                {chat.unread ? (
                  <View
                    style={{
                      backgroundColor: colors.accent,
                      minWidth: 22,
                      height: 22,
                      borderRadius: 11,
                      paddingHorizontal: 6,
                      alignItems: 'center',
                      justifyContent: 'center',
                    }}
                  >
                    <Text style={{ color: colors.onAccent, fontSize: 12, fontWeight: '700' }}>
                      {chat.unread > 99 ? '99+' : chat.unread}
                    </Text>
                  </View>
                ) : null}
              </View>
            </View>
          </Pressable>
        )}
        refreshing={false}
        onRefresh={() => {
          void store.refresh();
        }}
      />
    </View>
  );
}
