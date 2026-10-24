import { BottomSheet, RNHostView } from '@expo/ui';
import { ActivityIndicator, FlatList, ScrollView, Text, View } from 'react-native';
import { SettingsRows } from '../components/settings-rows';
import { EmptyState, IconButton, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { MutedChat, MutedChatsSession, MutedChatsState, MutedChatTicket } from './muted-chats';

export const mutedChatName = (chat: MutedChat) => chat.title || t('notifications.unnamedChat');

export function MutedChatsScreen({
  state,
  session,
  ticket,
  accountRequired,
  onOpen,
  onUnmute,
}: {
  state: MutedChatsState;
  session: MutedChatsSession;
  ticket: MutedChatTicket | null;
  accountRequired: boolean;
  onOpen(chat: MutedChat): void;
  onUnmute(chat: MutedChat): void;
}) {
  const { presentation: p } = useTheme();
  const selected = state.chats.find((chat) => chat.id === state.selected) ?? null;
  const disabled = state.phase !== 'ready' || state.pending !== null;
  return (
    <>
      <FlatList
        testID="muted-chats-screen"
        data={accountRequired || state.phase !== 'ready' ? [] : state.chats}
        keyExtractor={(chat) => chat.id}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{
          padding: 16,
          paddingBottom: 48,
          maxWidth: 640,
          width: '100%',
          alignSelf: 'center',
        }}
        refreshing={state.phase === 'loading'}
        onRefresh={() => void session.load(ticket)}
        ItemSeparatorComponent={() => <View style={{ height: 8 }} />}
        ListHeaderComponent={
          state.readFailed || state.actionFailed ? (
            <View style={{ gap: 12, paddingBottom: 16 }}>
              <Text selectable accessibilityRole="alert" style={{ color: p.destructive }}>
                {state.readFailed
                  ? t('notifications.mutedReadFailed')
                  : t('notifications.unmuteNotConfirmed')}
              </Text>
              <NativeButton
                testID="muted-chats-retry"
                label={t('chat.tryAgain2')}
                secondary
                disabled={state.phase === 'loading'}
                onPress={() => void session.load(ticket)}
              />
            </View>
          ) : undefined
        }
        ListEmptyComponent={
          accountRequired ? (
            <EmptyState
              title={t('settings.yourAccount')}
              detail={t('notifications.mutedAccountRequired')}
            />
          ) : state.phase === 'inactive' || state.phase === 'loading' ? (
            <ActivityIndicator
              accessibilityLabel={t('settings.pleaseWait')}
              color={p.list.accent}
            />
          ) : state.phase === 'ready' ? (
            <EmptyState title={t('notifications.noMutedChats')} />
          ) : undefined
        }
        renderItem={({ item }) => (
          <SettingsRows
            testID={`muted-chat-row-${item.id}`}
            rows={[
              {
                id: `muted-chat-${item.id}`,
                title: mutedChatName(item),
                icon: { ios: 'bell.slash', android: 'notifications_off', web: 'notifications_off' },
                hint: t('notifications.chatActions'),
                disabled,
                onPress: () => session.select(item.id, ticket),
              },
            ]}
          />
        )}
      />
      <BottomSheet
        testID="muted-chat-sheet"
        isPresented={Boolean(selected)}
        onDismiss={() => session.select(null, ticket)}
        snapPoints={['half', 'full']}
        containerColor={p.list.background}
      >
        <RNHostView>
          <ScrollView
            contentInsetAdjustmentBehavior="automatic"
            contentContainerStyle={{ gap: 24, paddingVertical: 16, paddingBottom: 32 }}
          >
            {selected ? (
              <>
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
                  <Text
                    accessibilityRole="header"
                    selectable
                    style={{ flex: 1, color: p.list.text, fontSize: 24, fontWeight: '600' }}
                  >
                    {mutedChatName(selected)}
                  </Text>
                  <IconButton
                    name={{ ios: 'xmark', android: 'close', web: 'close' }}
                    label={t('common.close')}
                    onPress={() => session.select(null, ticket)}
                  />
                </View>
                <NativeButton
                  testID="muted-chat-open"
                  label={t('notifications.openChat')}
                  secondary
                  disabled={disabled}
                  onPress={() => onOpen(selected)}
                />
                <NativeButton
                  testID="muted-chat-unmute"
                  label={state.pending ? t('settings.pleaseWait') : t('notifications.reenable')}
                  disabled={disabled}
                  onPress={() => onUnmute(selected)}
                />
              </>
            ) : null}
          </ScrollView>
        </RNHostView>
      </BottomSheet>
    </>
  );
}
