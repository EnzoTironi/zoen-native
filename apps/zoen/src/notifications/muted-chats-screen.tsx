import { ActivityIndicator, FlatList, Text, View } from 'react-native';
import { SettingsRows } from '../components/settings-rows';
import { EmptyState, NativeButton } from '../components/primitives';
import { MuteDurationSheet } from './mute-duration-sheet';
import { muteSummary } from './mute-presentation';
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
                  : t('notifications.muteNotConfirmed')}
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
                supportingText: muteSummary(item.muted_until_ms),
                icon: { ios: 'bell.slash', android: 'notifications_off', web: 'notifications_off' },
                hint: t('notifications.chatActions'),
                disabled,
                onPress: () => session.select(item.id, ticket),
              },
            ]}
          />
        )}
      />
      <MuteDurationSheet
        key={`${state.scope}:${selected?.id ?? ''}`}
        chat={selected}
        clock={state.clock_ms}
        busy={disabled}
        onDismiss={() => session.select(null, ticket)}
        onOpen={selected ? () => onOpen(selected) : undefined}
        onUnmute={() => {
          if (selected) onUnmute(selected);
        }}
        onChange={(choice) => {
          if (selected) void session.mute(selected.id, choice, ticket);
        }}
      />
    </>
  );
}
