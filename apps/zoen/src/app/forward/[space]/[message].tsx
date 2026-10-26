import { t, formatLocale } from '../../../i18n/index.ts';
import { ProfileAvatar } from '../../../profile/profile-avatar';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { Keyboard, Pressable, ScrollView, Text, TextInput, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { ForwardMessagePreview } from '../../../components/forward-message-preview';
import { useClient } from '../../../core/provider';
import {
  ForwardMessageSession,
  ForwardMessageUnavailable,
  ForwardMessageChanged,
  type ForwardMessageReview,
} from '../../../core/forward-message';
import { homeOwnerFor, homeSpacesFor } from '../../../home/access';
import { useTheme } from '../../../theme';
import { EmptyState, NativeButton } from '../../../components/primitives';

export default function Forward() {
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const previewOwner = useRef<ForwardMessageReview | null>(null);
  const { space, message } = useLocalSearchParams<{ space: string; message: string }>();
  const client = useClient();
  const { snapshot, store } = client;
  const { colors } = useTheme();
  const [query, setQuery] = useState('');
  const [review, setReview] = useState<ForwardMessageReview | null>(null);
  useLayoutEffect(() => {
    previewOwner.current = review;
    return () => {
      if (previewOwner.current === review) previewOwner.current = null;
    };
  }, [review]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const owner = homeOwnerFor(client);
  const identityId = owner?.identityId;
  const deviceId = owner?.deviceId;
  const mode = owner?.mode;
  const sessionEpoch = store.sessionEpoch;
  const session = useMemo(
    () =>
      new ForwardMessageSession(
        identityId && deviceId && mode
          ? {
              owner: { identityId, deviceId, mode },
              spaceId: space,
              messageId: message,
              sessionEpoch,
            }
          : null,
        {
          owner: () => homeOwnerFor(store.getSnapshot()),
          spaces: () => homeSpacesFor(store.getSnapshot()),
          epoch: () => store.sessionEpoch,
          writable: (target) =>
            store
              .getSnapshot()
              .snapshot?.conversations.some((chat) => chat.id === target && chat.can_write) ===
            true,
          status: (source) => store.driver.request({ op: 'forward_status', source }),
          review: (scope) => store.driver.request({ op: 'forward_review', scope }),
          confirm: (scope, operation_id, source_revision) =>
            store.request({ op: 'confirm_forward', scope, operation_id, source_revision }),
          dismiss: (scope, operation_id, accepted_message_id) =>
            store.driver.request({
              op: 'dismiss_forward',
              scope,
              operation_id,
              accepted_message_id,
            }),
        },
        {
          busy: setBusy,
          reviewed: setReview,
          error: (reason) =>
            setError(
              reason === null
                ? null
                : reason instanceof ForwardMessageChanged
                  ? t('chat.reviewForwardAgain')
                  : reason instanceof ForwardMessageUnavailable
                    ? t('chat.thisMessageIsNoLongerInThisDevice')
                    : reason instanceof Error
                      ? reason.message
                      : t('chat.theMessageCouldNotBeForwarded'),
            ),
          sent: () => router.back(),
        },
      ),
    [identityId, deviceId, mode, sessionEpoch, space, message, store],
  );
  useFocusEffect(
    useCallback(() => {
      void session.focus();
      return () => {
        previewOwner.current = null;
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
  }, [session, store]);
  const spaces = homeSpacesFor(client);
  const chats =
    snapshot?.conversations.filter(
      (chat) =>
        spaces.has(space) &&
        spaces.has(chat.id) &&
        chat.can_write &&
        chat.title.toLocaleLowerCase(formatLocale).includes(query.toLocaleLowerCase(formatLocale)),
    ) ?? [];
  const previewCurrent = useCallback(() => {
    const current = homeOwnerFor(store.getSnapshot());
    return (
      previewOwner.current === review &&
      review !== null &&
      store.sessionEpoch === sessionEpoch &&
      current?.identityId === identityId &&
      current?.deviceId === deviceId &&
      current?.mode === mode &&
      homeSpacesFor(store.getSnapshot()).has(space)
    );
  }, [review, store, sessionEpoch, identityId, deviceId, mode, space]);
  const target = review && snapshot?.conversations.find((chat) => chat.id === review.targetId);
  const reviewedMessage = review?.message;
  const content =
    reviewedMessage && 'Message' in reviewedMessage.kind ? reviewedMessage.kind.Message : null;
  if (mode !== 'account')
    return (
      <View style={{ flex: 1, paddingTop: headerHeight }}>
        <Stack.Screen options={{ title: t('chat.forwardTo') }} />
        <EmptyState title={t('chat.forwardRequiresAccount')}>
          <NativeButton
            testID="forward-message-use-account"
            label={t('settings.useYourOwnAccount')}
            onPress={() => {
              void store.open('account');
            }}
          />
        </EmptyState>
      </View>
    );
  return (
    <View style={{ flex: 1, paddingTop: headerHeight, backgroundColor: 'transparent' }}>
      <Stack.Screen options={{ title: t('chat.forwardTo') }} />
      {!review && (
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
      )}
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ padding: 20, color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      {review && content ? (
        <View
          testID="forward-message-review"
          style={{ flex: 1, padding: 20, paddingBottom: Math.max(20, insets.bottom), gap: 20 }}
        >
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 14 }}>
            <ProfileAvatar
              person={target?.counterpart}
              name={target?.title ?? t('chat.chatUnavailable')}
              color={target?.counterpart?.tint_hex}
            />
            <Text accessibilityRole="header" style={{ color: colors.text, fontSize: 20, flex: 1 }}>
              {target?.title ?? t('chat.chatUnavailable')}
            </Text>
          </View>
          <ScrollView
            keyboardShouldPersistTaps="handled"
            keyboardDismissMode="interactive"
            style={{ flex: 1 }}
            contentContainerStyle={{ gap: 8 }}
          >
            <Text style={{ color: colors.secondary }}>
              {snapshot?.conversations.find((chat) => chat.id === space)?.title}
            </Text>
            {reviewedMessage?.author?.name ? (
              <Text style={{ color: colors.text, fontWeight: '600' }}>
                {reviewedMessage.author.name}
              </Text>
            ) : null}
            <ForwardMessagePreview
              key={`${space}:${reviewedMessage!.id}`}
              spaceId={space}
              entry={reviewedMessage!}
              isCurrent={previewCurrent}
            />
          </ScrollView>
          {review.acceptedMessageId !== null ? (
            <Text
              testID="forward-message-accepted"
              accessibilityLiveRegion="polite"
              style={{ color: colors.secondary }}
            >
              {t('chat.alreadyForwarded')}
            </Text>
          ) : null}
          <NativeButton
            testID="forward-message-send"
            label={t(review.acceptedMessageId !== null ? 'chat.forwardAgain' : 'chat.send')}
            disabled={busy || (review.acceptedMessageId === null && target?.can_write !== true)}
            onPress={() => {
              if (review.acceptedMessageId !== null) void session.cancelReview();
              else void session.confirm(review);
            }}
          />
          <NativeButton
            testID="forward-message-cancel"
            label={t('common.cancel')}
            secondary
            disabled={busy}
            onPress={() => {
              previewOwner.current = null;
              if (review.acceptedMessageId !== null) router.back();
              else void session.cancelReview();
            }}
          />
        </View>
      ) : (
        <FlashList
          contentInsetAdjustmentBehavior="never"
          keyboardShouldPersistTaps="handled"
          keyboardDismissMode="interactive"
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
              accessibilityLabel={item.title}
              onPress={() => {
                Keyboard.dismiss();
                void session.prepare(item.id);
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
      )}
    </View>
  );
}
