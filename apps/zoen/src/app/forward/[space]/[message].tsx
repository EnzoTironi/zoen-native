import { t, formatLocale } from '../../../i18n/index.ts';
import { ProfileAvatar } from '../../../profile/profile-avatar';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { Pressable, Text, TextInput, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { ForwardMessageSession, ForwardMessageUnavailable } from '../../../core/forward-message';
import { homeOwnerFor, homeSpacesFor } from '../../../home/access';
import { useTheme } from '../../../theme';
import { EmptyState } from '../../../components/primitives';

export default function Forward() {
  const headerHeight = useHeaderHeight();
  const { space, message } = useLocalSearchParams<{ space: string; message: string }>();
  const client = useClient();
  const { snapshot, store } = client;
  const { colors } = useTheme();
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const owner = homeOwnerFor(client);
  const identityId = owner?.identityId;
  const deviceId = owner?.deviceId;
  const mode = owner?.mode;
  const session = useMemo(
    () =>
      new ForwardMessageSession(
        identityId && deviceId && mode
          ? { owner: { identityId, deviceId, mode }, spaceId: space, messageId: message }
          : null,
        {
          owner: () => homeOwnerFor(store.getSnapshot()),
          spaces: () => homeSpacesFor(store.getSnapshot()),
          message: (space_id, message_id) =>
            store.driver.request({ op: 'message', space_id, message_id }),
          forward: (source_space, message_id, target_space) =>
            store.request({ op: 'forward_message', source_space, message_id, target_space }),
        },
        {
          busy: setBusy,
          error: (reason) =>
            setError(
              reason === null
                ? null
                : reason instanceof ForwardMessageUnavailable
                  ? t('chat.thisMessageIsNoLongerInThisDevice')
                  : reason instanceof Error
                    ? reason.message
                    : t('chat.theMessageCouldNotBeForwarded'),
            ),
          sent: (id) => router.dismissTo({ pathname: '/chat/[id]', params: { id } }),
        },
      ),
    [identityId, deviceId, mode, space, message, store],
  );
  useFocusEffect(
    useCallback(() => {
      session.focus();
      return () => session.blur();
    }, [session]),
  );
  useEffect(() => {
    const unsubscribe = store.subscribe(() => session.refresh());
    return () => {
      unsubscribe();
      session.dispose();
    };
  }, [session, store]);
  const spaces = homeSpacesFor(client);
  const chats =
    snapshot?.conversations.filter(
      (chat) =>
        spaces.has(space) &&
        spaces.has(chat.id) &&
        chat.id !== space &&
        chat.title.toLocaleLowerCase(formatLocale).includes(query.toLocaleLowerCase(formatLocale)),
    ) ?? [];
  return (
    <View style={{ flex: 1, paddingTop: headerHeight, backgroundColor: 'transparent' }}>
      <Stack.Screen options={{ title: t('chat.forwardTo') }} />
      <TextInput
        accessibilityLabel={t('chat.searchChats2')}
        value={query}
        onChangeText={setQuery}
        placeholder={t('chat.searchChats2')}
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
        contentInsetAdjustmentBehavior="never"
        data={chats}
        keyExtractor={(chat) => chat.id}
        ListEmptyComponent={
          <EmptyState
            title={t('chat.noChatsFound')}
            detail={t('chat.startAnotherChatToForwardThisMessage')}
          />
        }
        renderItem={({ item }) => (
          <Pressable
            disabled={busy}
            accessibilityRole="button"
            accessibilityLabel={t('chat.forwardToNamedChat', { title: item.title })}
            onPress={() => {
              void session.send(item.id);
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
            <ProfileAvatar
              person={item.counterpart}
              name={item.title}
              color={item.counterpart?.tint_hex}
            />
            <Text style={{ color: colors.text, fontSize: 17, flex: 1 }}>{item.title}</Text>
          </Pressable>
        )}
      />
    </View>
  );
}
