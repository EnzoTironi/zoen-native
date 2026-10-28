import { useCallback, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { Text, TextInput, View } from 'react-native';
import { ChatDestinations } from '../../../components/chat-destinations';
import { useClient } from '../../../core/provider';
import { prepareSharedReply } from '../../../core/shared-reply';
import { homeOwnerFor, homeSpacesFor } from '../../../home/access';
import { conversationTitle } from '../../../saved-messages/presentation';
import { t, formatLocale } from '../../../i18n';
import { useTheme } from '../../../theme';

export default function ReplyDestination() {
  const { space, message, revision } = useLocalSearchParams<{
    space: string;
    message: string;
    revision: string;
  }>();
  const client = useClient();
  const { store, snapshot } = client;
  const { colors } = useTheme();
  const headerHeight = useHeaderHeight();
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false);
  const focused = useRef(false);
  const epoch = store.sessionEpoch;
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
      };
    }, []),
  );
  const spaces = homeSpacesFor(client);
  const chats =
    snapshot?.conversations.filter(
      (chat) =>
        spaces.has(space) &&
        spaces.has(chat.id) &&
        chat.id !== space &&
        chat.can_write &&
        conversationTitle(chat)
          .toLocaleLowerCase(formatLocale)
          .includes(query.toLocaleLowerCase(formatLocale)),
    ) ?? [];
  async function choose(target: string) {
    if (pending.current || !focused.current) return;
    const owner = homeOwnerFor(store.getSnapshot());
    const channel = store.messageDrafts.channel(target, null);
    if (!channel || owner?.mode !== 'account' || !store.driver.durableMessageDrafts) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    const current = () =>
      focused.current &&
      epoch === store.sessionEpoch &&
      store.messageDrafts.isCurrent(channel) &&
      homeSpacesFor(store.getSnapshot()).has(space) &&
      store.getSnapshot().snapshot?.conversations.some((c) => c.id === target && c.can_write) ===
        true;
    try {
      const draftRevision = await prepareSharedReply({
        channel,
        current,
        review: () =>
          store.driver.request({
            op: 'review_shared_reply',
            expected_identity_id: owner.identityId,
            expected_device_id: owner.deviceId,
            space_id: target,
            thread_root_id: null,
            source_space_id: space,
            message_id: message,
            expected_source_revision: revision,
          }),
      });
      if (current())
        router.dismissTo({
          pathname: '/chat/[id]',
          params: { id: target, compose: draftRevision },
        });
    } catch (reason) {
      if (current())
        setError(reason instanceof Error ? reason.message : t('chat.messageUnavailable'));
    } finally {
      pending.current = false;
      if (focused.current) setBusy(false);
    }
  }
  return (
    <View style={{ flex: 1, paddingTop: headerHeight }}>
      <Stack.Screen options={{ title: t('chat.replyInAnotherChat') }} />
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
        <Text accessibilityRole="alert" style={{ padding: 20, color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <ChatDestinations
        chats={chats}
        busy={busy}
        onChoose={(target) => {
          void choose(target);
        }}
      />
    </View>
  );
}
