import { t } from '../../../i18n/index.ts';
import { Stack, router } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { useMemo, useState } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { useTheme } from '../../../theme';
import { EmptyState } from '../../../components/primitives';
import { InboxChatRow } from '../../../inbox/chat-row';
import { useInboxActions } from '../../../inbox/use-inbox-actions';
import { InboxActionFeedback } from '../../../inbox/action-feedback';
import { ChatsActions } from '../../../components/chats-actions';
import { useHome } from '../../../home/provider';
import { MiniappRail } from '../../../miniapps/rail';
import { AppCardEditControls } from '../../../miniapps/card-edit-controls';

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
  const { presentation: p } = useTheme();
  const home = useHome();
  const inbox = useInboxActions();
  const { items } = home;
  const [filter, setFilter] = useState<string>('all');
  const [cardPending, setCardPending] = useState(false);
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
                paddingHorizontal: 16,
                paddingBottom: 6,
                flexDirection: 'row',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 4,
              }}
            >
              <Text
                testID="chats-title"
                accessibilityRole="header"
                numberOfLines={1}
                style={{ ...p.typography.largeTitle, color: p.text, flexShrink: 1 }}
              >
                {t('chat.chats')}
              </Text>
              <View style={{ flex: 1, alignItems: 'flex-end' }}>
                {home.editing ? (
                  <AppCardEditControls
                    pending={cardPending || home.editSaving}
                    onDone={() => {
                      void home.commitEdit().catch(() => {});
                    }}
                    onDiscard={home.discardEdit}
                  />
                ) : null}
              </View>
              <ChatsActions />
            </View>
            <MiniappRail
              key={home.ownerKey}
              items={items}
              paddingVertical={2}
              editing={home.editing}
              onEditingChange={(value) => (value ? home.beginEdit() : home.discardEdit())}
              onPendingChange={setCardPending}
              onReorder={(next, expected) => {
                home.requireEdit();
                return home.reorderItems(next, expected);
              }}
              error={home.error}
              onRemove={(item) => {
                home.requireEdit();
                return home.hide(item.id);
              }}
              onTogglePin={(item) => {
                home.requireEdit();
                return home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id);
              }}
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
              contentContainerStyle={{ gap: 24, paddingHorizontal: 16, paddingTop: 8 }}
            >
              {filters.map((item) => (
                <Pressable
                  key={item.id}
                  testID={`inbox-filter-${item.id}`}
                  accessibilityRole="button"
                  accessibilityState={{ selected: filter === item.id }}
                  aria-pressed={process.env.EXPO_OS === 'web' ? filter === item.id : undefined}
                  onPress={() => setFilter(item.id)}
                  style={({ pressed }) => ({
                    minHeight: 44,
                    paddingHorizontal: 0,
                    justifyContent: 'center',
                    backgroundColor: 'transparent',
                    opacity: pressed ? 0.65 : 1,
                  })}
                >
                  <Text
                    style={{
                      color: filter === item.id ? p.accent : p.chatList.message,
                      fontSize: 15,
                      fontWeight: '600',
                    }}
                  >
                    {item.label}
                  </Text>
                  {filter === item.id ? (
                    <View
                      style={{
                        position: 'absolute',
                        bottom: 0,
                        left: 0,
                        right: 0,
                        height: 3,
                        borderRadius: 1.5,
                        backgroundColor: p.accent,
                      }}
                    />
                  ) : null}
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
          <InboxChatRow
            chat={chat}
            enabled={inbox.enabled && !home.editing}
            pending={inbox.state.pending !== null}
            onOpen={() => {
              if (inbox.canOpen(chat.id))
                router.push(
                  chat.kind === 'saved' && !chat.can_write
                    ? '/saved-messages'
                    : { pathname: '/chat/[id]', params: { id: chat.id } },
                );
            }}
            onAction={(action) => {
              void inbox.session.run(chat, action, inbox.ticket);
            }}
          />
        )}
        refreshing={false}
        onRefresh={() => {
          void store.refresh();
        }}
      />
      <InboxActionFeedback state={inbox.state} session={inbox.session} ticket={inbox.ticket} />
    </View>
  );
}
