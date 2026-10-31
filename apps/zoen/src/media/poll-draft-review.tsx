import { router } from 'expo-router';
import { Text, View } from 'react-native';
import { IconButton, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { AttachmentReplyContext } from './attachment-reply-context';
import { copyMediaReply, sameMediaReply } from './media-reply';
import type { MediaDraftPoll } from './media-draft-types';
import type { useDurableAttachmentController } from './use-durable-attachment-controller';

/** Recovery only. Publishing remains the sheet's explicit Create action. */
export function renderPollDraftReview(
  context: ReturnType<typeof useDurableAttachmentController>,
  poll: MediaDraftPoll,
) {
  const { channel, state, runtime, options, current, store, colors, failure, cancel, own } =
    context;
  if (!channel || !state) return null;
  const live = () =>
    current() && runtime.foreground && channel.getSnapshot().content?.kind === 'poll';
  const disabled = options.disabled || options.suspended || state.sending || Boolean(own?.clearing);
  return (
    <View testID="poll-draft-review" style={{ gap: 8, paddingVertical: 8 }}>
      <AttachmentReplyContext
        scope={{
          mode: 'account',
          identityId: channel.scope.expected_identity_id,
          deviceId: channel.scope.expected_device_id,
          spaceId: channel.scope.space_id,
          thread: Boolean(channel.scope.thread_root_id),
          replyTo: poll.reply_to ?? channel.scope.thread_root_id,
          replyQuote: poll.reply_quote,
          sharedReply: poll.shared_reply,
          replyIntent: poll.reply_intent,
        }}
        onCancel={
          !disabled &&
          !state.frozen &&
          (poll.shared_reply ||
            poll.reply_quote ||
            (poll.reply_to && !channel.scope.thread_root_id))
            ? () => {
                if (!live()) return;
                const latest = channel.getSnapshot();
                const next = latest.content;
                if (
                  latest.frozen ||
                  latest.sending ||
                  next?.kind !== 'poll' ||
                  next.reply_to !== poll.reply_to ||
                  !sameMediaReply(next, poll)
                )
                  return;
                const previous = { reply_to: next.reply_to, ...copyMediaReply(next) };
                void channel
                  .save({
                    ...next,
                    reply_to: null,
                    reply_quote: undefined,
                    shared_reply: undefined,
                    reply_intent: undefined,
                  })
                  .catch(failure);
                options.onReplyCancelled?.(previous);
              }
            : undefined
        }
      />
      <View style={{ flexDirection: 'row', gap: 8, alignItems: 'center', paddingHorizontal: 16 }}>
        <Text numberOfLines={2} style={{ flex: 1, color: colors.text, fontSize: 16 }}>
          {poll.question.trim() || t('attachmentTabs.poll')}
        </Text>
        <NativeButton
          secondary
          label={t('attachmentTabs.resumePoll')}
          disabled={disabled}
          onPress={() => {
            if (!live() || channel.getSnapshot().sending || runtime.routeOpening) return;
            runtime.routeOpening = true;
            router.push({
              pathname: '/chat-attachment/[id]',
              params: {
                id: channel.scope.space_id,
                kind: 'poll',
                thread: channel.scope.thread_root_id ?? '',
                identity: channel.scope.expected_identity_id,
                device: channel.scope.expected_device_id,
                epoch: String(store.sessionEpoch),
              },
            });
          }}
        />
        <IconButton
          label={t('chat.cancel')}
          name={{ ios: 'xmark', android: 'close', web: 'close' }}
          disabled={disabled}
          onPress={() => {
            if (live()) void cancel();
          }}
        />
      </View>
      {state.error || own?.error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, paddingHorizontal: 16 }}>
          {own?.error ?? state.error?.message}
        </Text>
      ) : null}
    </View>
  );
}
