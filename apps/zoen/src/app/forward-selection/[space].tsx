import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { FlashList } from '@shopify/flash-list';
import { Keyboard, Pressable, ScrollView, Text, TextInput, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { ForwardMessagePreview } from '../../components/forward-message-preview';
import { EmptyState, NativeButton } from '../../components/primitives';
import { selectionMessageIds } from '../../core/message-selection';
import { conversationScope } from '../../core/conversation-scope';
import { ForwardSelectionSession, type ForwardSelectionReview } from '../../core/forward-selection';
import { useClient } from '../../core/provider';
import { homeOwnerFor, homeSpacesFor } from '../../home/access';
import { t, formatLocale } from '../../i18n/index.ts';
import { ConversationAvatar } from '../../saved-messages/conversation-avatar';
import { conversationTitle } from '../../saved-messages/presentation';
import { useTheme } from '../../theme';

/** One reviewed set and destination; Core owns each source's operation and receipt. */
export default function ForwardSelection() {
  const { space, messages, selectionScope } = useLocalSearchParams<{
    space: string;
    messages: string;
    selectionScope: string;
  }>();
  const ids = useMemo(() => selectionMessageIds(messages), [messages]);
  const client = useClient();
  const { store, snapshot } = client;
  const { colors } = useTheme();
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const owner = homeOwnerFor(client);
  const identityId = owner?.identityId;
  const deviceId = owner?.deviceId;
  const mode = owner?.mode;
  const epoch = store.sessionEpoch;
  const available = Boolean(
    selectionScope &&
    ids.length &&
    mode === 'account' &&
    conversationScope(client, epoch, space) === selectionScope,
  );
  const [query, setQuery] = useState('');
  const [review, setReview] = useState<ForwardSelectionReview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const preview = useRef<ForwardSelectionReview | null>(null);
  const focused = useRef(false);
  useLayoutEffect(() => {
    preview.current = review;
    return () => {
      preview.current = null;
    };
  }, [review]);
  const session = useMemo(
    () =>
      new ForwardSelectionSession(
        available && identityId && deviceId && mode
          ? {
              owner: { identityId, deviceId, mode },
              spaceId: space,
              messageIds: ids,
              sessionEpoch: epoch,
            }
          : null,
        {
          owner: () => (store.isSessionRetiring ? null : homeOwnerFor(store.getSnapshot())),
          spaces: () => homeSpacesFor(store.getSnapshot()),
          epoch: () => store.sessionEpoch,
          writable: (target) =>
            store
              .getSnapshot()
              .snapshot?.conversations.some((chat) => chat.id === target && chat.can_write) ===
            true,
          status: (source) => store.driver.request({ op: 'forward_batch_status', source }),
          review: (scope) => store.driver.request({ op: 'forward_batch_review', scope }),
          confirm: (source, batch_id, review_revision) =>
            store.request({ op: 'confirm_forward_batch', source, batch_id, review_revision }),
          abandon: (source, batch_id, review_revision) =>
            store.driver.request({
              op: 'abandon_forward_batch',
              source,
              batch_id,
              review_revision,
            }),
          finish: (source, batch_id) =>
            store.driver.request({ op: 'finish_forward_batch', source, batch_id }),
        },
        {
          busy: setBusy,
          reviewed: setReview,
          error: (reason) =>
            setError(
              reason === null
                ? null
                : reason instanceof Error
                  ? reason.message
                  : t('chat.theMessageCouldNotBeForwarded'),
            ),
          sent: () => router.back(),
        },
      ),
    [available, identityId, deviceId, mode, space, ids, epoch, store],
  );
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      void session.focus();
      return () => {
        focused.current = false;
        preview.current = null;
        session.blur();
      };
    }, [session]),
  );
  useEffect(() => {
    const unsubscribe = store.subscribe(() => session.refresh());
    return () => {
      unsubscribe();
      session.dispose();
    };
  }, [store, session]);
  const current = () =>
    focused.current &&
    preview.current === review &&
    review !== null &&
    !store.isSessionRetiring &&
    conversationScope(store.getSnapshot(), store.sessionEpoch, space) === selectionScope;
  const spaces = homeSpacesFor(client);
  const chats =
    snapshot?.conversations.filter(
      (chat) =>
        spaces.has(space) &&
        spaces.has(chat.id) &&
        chat.can_write &&
        conversationTitle(chat)
          .toLocaleLowerCase(formatLocale)
          .includes(query.toLocaleLowerCase(formatLocale)),
    ) ?? [];
  const destinationTitle = (id: string) => {
    const chat = snapshot?.conversations.find((chat) => chat.id === id);
    return chat ? conversationTitle(chat) : t('chat.chatUnavailable');
  };
  const target = review && snapshot?.conversations.find((chat) => chat.id === review.targetId);
  const accepted =
    review?.messages.filter((message) => message.acceptedMessageId !== null).length ?? 0;
  const skippedIds = review?.skippedIds ?? [];
  const complete = Boolean(review && accepted + skippedIds.length === ids.length);
  return (
    <View style={{ flex: 1, paddingTop: headerHeight, paddingBottom: insets.bottom }}>
      <Stack.Screen options={{ title: t('chat.forwardTo') }} />
      {!available ? (
        <EmptyState title={t('chat.thisMessageIsNoLongerInThisDevice')} />
      ) : (
        <>
          {error ? (
            <Text accessibilityRole="alert" style={{ color: colors.danger, padding: 16 }}>
              {error}
            </Text>
          ) : null}
          {review ? (
            <>
              <View style={{ padding: 16, gap: 8 }}>
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={`${t('chat.forwardTo')}: ${destinationTitle(review.targetId)}`}
                  disabled={busy}
                  onPress={() => {
                    if (current()) {
                      preview.current = null;
                      session.clearReview();
                      setReview(null);
                    }
                  }}
                >
                  <Text
                    accessibilityRole="header"
                    style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}
                  >
                    {target ? conversationTitle(target) : t('chat.chatUnavailable')}
                  </Text>
                </Pressable>
                <Text accessibilityLiveRegion="polite" style={{ color: colors.secondary }}>
                  {t('chat.forwardSelectionProgress', { accepted, count: ids.length })}
                  {skippedIds.length
                    ? ` · ${t('chat.forwardSelectionSkipped', { count: skippedIds.length })}`
                    : ''}
                </Text>
              </View>
              <ScrollView
                testID="forward-selection-review"
                contentContainerStyle={{ padding: 16, gap: 20 }}
              >
                {review.messages.map((message) => (
                  <View key={message.message.id} style={{ gap: 8 }}>
                    <Text style={{ color: colors.secondary }}>{message.message.author.name}</Text>
                    <ForwardMessagePreview
                      spaceId={space}
                      entry={message.message}
                      isCurrent={current}
                    />
                    {skippedIds.includes(message.message.id) ? (
                      <Text style={{ color: colors.secondary }}>{t('chat.notForwarded')}</Text>
                    ) : null}
                    {message.acceptedMessageId !== null ? (
                      <Text style={{ color: colors.secondary }}>
                        {t('chat.alreadyForwarded')}
                        {message.targetId !== review.targetId
                          ? ` · ${destinationTitle(message.targetId)}`
                          : ''}
                      </Text>
                    ) : null}
                  </View>
                ))}
              </ScrollView>
              {!review.ready && !review.completed && !complete ? (
                <View style={{ paddingHorizontal: 16 }}>
                  <NativeButton
                    testID="forward-selection-stop"
                    label={t('chat.stopRemainingForwarding')}
                    secondary
                    disabled={busy}
                    onPress={() => {
                      if (current()) void session.abandon(review);
                    }}
                  />
                </View>
              ) : null}
              <View style={{ padding: 16, gap: 12, flexDirection: 'row' }}>
                <View style={{ flex: 1 }}>
                  <NativeButton
                    label={t(review.completed ? 'common.close' : 'chat.cancel')}
                    fullWidth
                    secondary
                    disabled={busy}
                    onPress={() => {
                      if (current()) router.back();
                    }}
                  />
                </View>
                <View style={{ flex: 1 }}>
                  <NativeButton
                    testID="forward-selection-send"
                    label={t(
                      review.completed
                        ? 'chat.forwardAgain'
                        : complete
                          ? 'common.close'
                          : review.ready
                            ? 'chat.send'
                            : 'chat.forwardSelectionReview',
                    )}
                    fullWidth
                    disabled={
                      busy || (!review.completed && !complete && target?.can_write !== true)
                    }
                    onPress={() => {
                      if (!current()) return;
                      if (review.completed) {
                        preview.current = null;
                        session.clearReview();
                        setReview(null);
                      } else if (complete) void session.finish(review);
                      else if (review.ready) void session.confirm(review);
                      else void session.prepare(review.targetId);
                    }}
                  />
                </View>
              </View>
            </>
          ) : (
            <>
              <TextInput
                value={query}
                onChangeText={setQuery}
                placeholder={t('chat.searchChats2')}
                accessibilityLabel={t('chat.searchChats2')}
                style={{ padding: 16, color: colors.text, fontSize: 17 }}
              />
              <FlashList
                data={chats}
                keyExtractor={(chat) => chat.id}
                keyboardShouldPersistTaps="handled"
                renderItem={({ item }) => (
                  <Pressable
                    disabled={busy}
                    accessibilityRole="button"
                    accessibilityLabel={conversationTitle(item)}
                    style={{
                      flexDirection: 'row',
                      alignItems: 'center',
                      gap: 14,
                      paddingHorizontal: 24,
                      paddingVertical: 16,
                    }}
                    onPress={() => {
                      Keyboard.dismiss();
                      void session.prepare(item.id);
                    }}
                  >
                    <ConversationAvatar chat={item} />
                    <Text style={{ color: colors.text, fontSize: 17 }}>
                      {conversationTitle(item)}
                    </Text>
                  </Pressable>
                )}
                ListEmptyComponent={<EmptyState title={t('chat.noChatsFound')} />}
              />
            </>
          )}
        </>
      )}
    </View>
  );
}
