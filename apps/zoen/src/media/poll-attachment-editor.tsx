import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Text, View } from 'react-native';
import { randomUUID } from 'expo-crypto';
import { Loading, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { AttachmentReplyContext } from './attachment-reply-context';
import { PollAttachmentFields } from './poll-attachment-fields';
import { PollDraftOwner } from './poll-draft-owner';
import type { MediaDraftChannel } from './media-draft-channel';
import type { MediaReplyTarget } from './media-reply';

type Props = {
  channel: MediaDraftChannel;
  reply: MediaReplyTarget;
  current(): boolean;
  onCreated(): void;
  onReplyCancelled?(reply: MediaReplyTarget): void;
};

export function PollAttachmentEditor({
  channel,
  reply,
  current,
  onCreated,
  onReplyCancelled,
}: Props) {
  const { colors } = useTheme();
  const [owner] = useState(() => new PollDraftOwner(channel, reply, current, randomUUID));
  const state = useSyncExternalStore(channel.subscribe, channel.getSnapshot, channel.getSnapshot);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const feedback = useRef(0);
  useEffect(() => {
    owner.activate();
    void channel.load().catch(() => {});
    return () => owner.retire();
  }, [owner, channel]);
  function failureMessage(reason: unknown) {
    const invalid =
      reason &&
      typeof reason === 'object' &&
      (('kind' in reason && reason.kind === 'invalid') ||
        ('message' in reason && reason.message === 'poll-invalid'));
    return invalid
      ? t('attachmentTabs.pollLimit')
      : (channel.getSnapshot().error?.message ?? t('attachmentTabs.failed'));
  }
  function edit(work: () => Promise<unknown>) {
    if (!current()) return;
    const token = ++feedback.current;
    setError(null);
    const failed = (reason: unknown) => {
      if (current() && feedback.current === token) setError(failureMessage(reason));
    };
    try {
      void work().catch(failed);
    } catch (reason) {
      failed(reason);
    }
  }
  async function submit() {
    if (!current()) return;
    const token = ++feedback.current;
    setError(null);
    setSubmitting(true);
    try {
      await owner.submit();
      if (current()) onCreated();
    } catch (reason) {
      if (current() && feedback.current === token) setError(failureMessage(reason));
    } finally {
      if (current()) setSubmitting(false);
    }
  }
  const poll = state.content?.kind === 'poll' ? state.content : owner.initial;
  const blocked =
    state.phase !== 'ready' || Boolean(state.content && state.content.kind !== 'poll');
  const failure = error ?? state.error?.message;
  return (
    <View testID="poll-draft-editor" style={{ gap: 16 }}>
      {state.phase === 'loading' && !state.error ? (
        <Loading />
      ) : blocked ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t('attachmentTabs.unavailable')}
        </Text>
      ) : (
        <>
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
              !state.frozen &&
              !submitting &&
              (poll.shared_reply ||
                poll.reply_quote ||
                (poll.reply_to && !channel.scope.thread_root_id))
                ? () => {
                    edit(() => {
                      const cleared = owner.clearReply();
                      if (!cleared) return Promise.resolve();
                      onReplyCancelled?.(cleared.previous);
                      return cleared.saved;
                    });
                  }
                : undefined
            }
          />
          <PollAttachmentFields
            question={poll.question}
            options={poll.options}
            locked={state.frozen || submitting}
            sending={state.sending || submitting}
            retry={state.frozen && !state.reviewing}
            setQuestion={(value) => edit(() => owner.setQuestion(value))}
            setOption={(id, value) => edit(() => owner.setOption(id, value))}
            addOption={() => edit(() => owner.addOption())}
            removeOption={(id) => edit(() => owner.removeOption(id))}
            submit={() => {
              void submit();
            }}
          />
        </>
      )}
      {failure ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure}
        </Text>
      ) : null}
      {state.error && !state.frozen ? (
        <NativeButton
          secondary
          label={t('documents.tryAgain')}
          onPress={() => edit(() => channel.retry())}
        />
      ) : null}
    </View>
  );
}
