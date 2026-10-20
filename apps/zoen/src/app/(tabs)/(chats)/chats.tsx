import { t, formatLocale } from '../../../i18n/index.ts';
import { ProfileAvatar } from '../../../profile/profile-avatar';
import { Stack, router } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { useMemo, useState } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { useTheme } from '../../../theme';
import { EmptyState } from '../../../components/primitives';
import { ChatsActions } from '../../../components/chats-actions';
import { useHome } from '../../../home/provider';
import { MiniappRail } from '../../../miniapps/rail';

const filters = [
  { id: 'all', label: t('chat.all') },
  { id: 'unread', label: t('chat.unread') },
  { id: 'Direct', label: t('chat.people') },
  { id: 'Group', label: t('chat.groups') },
  { id: 'Community', label: t('chat.communities') },
  { id: 'archived', label: t('chat.archived') },
] as const;
export default function Chats() {
  const { snapshot, store } = useClient();
  const { colors } = useTheme();
  const home = useHome();
  const { items } = home;
  const [filter, setFilter] = useState<string>('all');
  const conversations = useMemo(
    () =>
      (snapshot?.conversations ?? [])
        .filter(
          (chat) =>
            (filter === 'archived' ? chat.archived : !chat.archived) &&
            (filter === 'all' ||
              filter === 'archived' ||
              (filter === 'unread' ? chat.unread > 0 : chat.kind === filter)),
        )
        .sort(
          (a, b) =>
            Number(b.pinned) - Number(a.pinned) || Number(b.last_at_ms) - Number(a.last_at_ms),
        ),
    [snapshot, filter],
  );
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      <Stack.Screen
        options={{
          title: t('chat.chats'),
          headerShown: false,
        }}
      />
      <FlashList
        style={{ backgroundColor: 'transparent' }}
        ListHeaderComponentStyle={{ backgroundColor: 'transparent' }}
        data={conversations}
        keyExtractor={(chat) => chat.id}
        // Inbox rows reorder on activity; anchoring their identities scrolls the header away.
        maintainVisibleContentPosition={{ disabled: true }}
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="on-drag"
        ListHeaderComponent={
          <View style={{ paddingBottom: 12, backgroundColor: 'transparent' }}>
            <View
              style={{
                paddingTop: 18,
                paddingHorizontal: 20,
                paddingBottom: 6,
                flexDirection: 'row',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 16,
              }}
            >
              <Text
                accessibilityRole="header"
                style={{ color: colors.text, fontSize: 34, fontWeight: '700', letterSpacing: -1 }}
              >
                {t('chat.chats')}
              </Text>
              <ChatsActions />
            </View>
            <MiniappRail
              key={home.ownerKey}
              items={items}
              onReorder={home.reorderItems}
              error={home.error}
              onRemove={(item) => home.hide(item.id)}
              onTogglePin={(item) =>
                home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id)
              }
              isPinned={home.isPinned}
              restoreLabel={t('home.restoreHiddenCount', { count: home.hiddenItems.length })}
              onRestore={
                home.hiddenItems.length
                  ? () => {
                      void home.restoreAll().catch(() => {});
                    }
                  : undefined
              }
            />
            <ScrollView
              horizontal
              style={{ backgroundColor: 'transparent' }}
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
                  style={({ pressed }) => ({
                    minHeight: 36,
                    paddingHorizontal: 14,
                    justifyContent: 'center',
                    borderRadius: 18,
                    backgroundColor: filter === item.id ? colors.text : 'transparent',
                    borderWidth: 1,
                    borderColor: filter === item.id ? colors.text : colors.line,
                    opacity: pressed ? 0.65 : 1,
                  })}
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
            title={t('chat.startAConversation')}
            detail={t('chat.talkToSomeoneBringAGroupTogetherOr')}
          />
        }
        renderItem={({ item: chat }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={
              chat.unread
                ? t('chat.conversationUnreadLabel', {
                    title: chat.title,
                    count: chat.unread,
                    preview: chat.last_preview,
                  })
                : t('chat.conversationLabel', { title: chat.title, preview: chat.last_preview })
            }
            onPress={() => router.push({ pathname: '/chat/[id]', params: { id: chat.id } })}
            style={({ pressed }) => ({
              flexDirection: 'row',
              gap: 14,
              paddingHorizontal: 20,
              paddingVertical: 14,
              minHeight: 88,
              alignItems: 'center',
              backgroundColor: 'transparent',
              opacity: pressed ? 0.65 : 1,
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
                    ? new Date(Number(chat.last_at_ms)).toLocaleTimeString(formatLocale, {
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
