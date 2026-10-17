import { t } from '../../../i18n/index.ts';
import { useCallback, useReducer, useRef, useState } from 'react';
import { router, Stack, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import { Text, View, type TextInputInstance } from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useCoreQuery } from '../../../core/query';
import { useClient } from '../../../core/provider';
import type { TimelineEntry } from '../../../core/types';
import { messageText } from '../../../core/presentation';
import { messageCard } from '../../../core/message-presentation';
import {
  initialMessageDraft,
  messageDraftReducer,
  captureMessageDraft,
} from '../../../core/message-draft';
import { EmptyState, Loading, NativeButton } from '../../../components/primitives';
import { useTheme } from '../../../theme';
import { useHome } from '../../../home/provider';
import { MessageComposer } from '../../../components/message-composer';
import { MessageContext } from '../../../components/message-context';
import { ConversationViewport } from '../../../components/conversation-viewport';
import { ConversationMessage } from '../../../components/conversation-message';
import { useMessageActions } from '../../../components/use-message-actions';

export default function Thread() {
  const { space, root } = useLocalSearchParams<{ space: string; root: string }>();
  const { store, notice } = useClient();
  const headerHeight = useHeaderHeight();
  const { top: statusBarHeight } = useSafeAreaInsets();
  const { items } = useHome();
  const input = useRef<TextInputInstance>(null);
  const list = useRef<FlashListRef<TimelineEntry>>(null);
  const sending = useRef(false);
  useFocusEffect(useCallback(() => () => store.setTyping(space, false), [space, store]));
  const {
    data: entries,
    error,
    retry,
  } = useCoreQuery({ op: 'thread', space_id: space, root_id: root });
  const { colors } = useTheme();
  const [messageDraft, updateDraft] = useReducer(
    messageDraftReducer,
    undefined,
    initialMessageDraft,
  );
  const { text: draft, reply, editing, editText: editDraft } = messageDraft;
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const actions = useMessageActions({
    spaceId: space,
    entries: entries ?? [],
    onReply: (entry) => {
      updateDraft({ type: 'reply', entry });
      input.current?.focus();
    },
    onEdit: (entry) => {
      updateDraft({ type: 'edit', entry, text: messageText(entry) });
      input.current?.focus();
    },
    onThread: () => input.current?.focus(),
  });
  async function send() {
    const attempt = captureMessageDraft(messageDraft);
    const text = attempt.text;
    if (!text.trim() || sending.current) return;
    sending.current = true;
    store.setTyping(space, false);
    setBusy(true);
    setFailure(null);
    try {
      if (attempt.editingId)
        await store.request({
          op: 'edit_message',
          space_id: space,
          message_id: attempt.editingId,
          text,
        });
      else
        await store.request({
          op: 'send_message',
          space_id: space,
          text,
          reply_to: attempt.replyId ?? root,
          thread: true,
        });
      updateDraft({ type: 'accepted', attempt });
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : t('chat.theReplyCouldNotBeSent'));
    } finally {
      sending.current = false;
      setBusy(false);
    }
  }
  return (
    <KeyboardAvoidingView
      behavior="padding"
      automaticOffset
      style={{ flex: 1, backgroundColor: 'transparent' }}
    >
      <Stack.Screen options={{ title: t('chat.replies') }} />
      <ConversationViewport
        composer={
          <>
            {failure || error || notice ? (
              <Text
                accessibilityRole="alert"
                selectable
                style={{ color: colors.danger, padding: 16 }}
              >
                {failure ?? error ?? notice}
              </Text>
            ) : null}
            {reply || editing ? (
              <MessageContext
                entry={(editing ?? reply)!}
                editing={Boolean(editing)}
                onCancel={() => {
                  updateDraft({ type: 'cancel' });
                }}
              />
            ) : null}
            <MessageComposer
              threadRootId={root}
              spaceId={space}
              inputRef={input}
              replyTo={reply?.id ?? root}
              thread
              editing={Boolean(editing)}
              value={editing ? editDraft : draft}
              sending={busy}
              onChangeText={(text) => {
                updateDraft({ type: 'text', value: text });
                store.setTyping(space, !editing && Boolean(text.trim()));
              }}
              onBlur={() => store.setTyping(space, false)}
              onSend={() => {
                void send();
              }}
              onMediaSent={() =>
                updateDraft({
                  type: 'media-accepted',
                  contextRevision: messageDraft.contextRevision,
                })
              }
            />
          </>
        }
      >
        {(composerSpacer) =>
          error && !entries ? (
            <EmptyState title={t('chat.theRepliesCouldNotBeOpened')} detail={error}>
              <NativeButton label={t('chat.tryAgain2')} onPress={retry} />
            </EmptyState>
          ) : !entries ? (
            <Loading />
          ) : (
            <FlashList
              style={{ flex: 1, marginTop: statusBarHeight, overflow: 'hidden' }}
              ref={list}
              data={entries}
              keyExtractor={(entry) => entry.id}
              keyboardDismissMode="interactive"
              keyboardShouldPersistTaps="handled"
              contentInsetAdjustmentBehavior="never"
              ListFooterComponent={composerSpacer}
              contentContainerStyle={{
                paddingHorizontal: 16,
                paddingTop: Math.max(0, headerHeight - statusBarHeight) + 8,
                paddingBottom: 12,
              }}
              maintainVisibleContentPosition={{
                startRenderingFromBottom: false,
                autoscrollToBottomThreshold: 0.15,
              }}
              renderItem={({ item: entry, index }) => (
                <View>
                  <ConversationMessage
                    spaceId={space}
                    entry={entry}
                    previous={index > 1 ? entries[index - 1] : undefined}
                    next={index > 0 ? entries[index + 1] : undefined}
                    groupChat
                    item={items.find((item) => item.id === messageCard(entry)?.item_id)}
                    reactionPending={actions.busy}
                    onReply={actions.reply}
                    onThread={actions.thread}
                    onMenu={actions.open}
                    onReact={(message, emoji) => {
                      void actions.react(message, emoji);
                    }}
                    onQuote={(messageId) => {
                      const target = entries.findIndex((message) => message.id === messageId);
                      if (target >= 0)
                        void list.current
                          ?.scrollToIndex({ index: target, animated: true, viewPosition: 0.5 })
                          .catch(() => {});
                      else
                        router.push({
                          pathname: '/chat/[id]',
                          params: { id: space, jump: messageId },
                        });
                    }}
                  />
                  {index === 0 ? (
                    <Text
                      style={{
                        color: colors.secondary,
                        fontSize: 12,
                        textAlign: 'center',
                        paddingVertical: 18,
                      }}
                    >
                      {t('chat.originalMessage')}
                    </Text>
                  ) : null}
                </View>
              )}
            />
          )
        }
      </ConversationViewport>
      {actions.panel}
    </KeyboardAvoidingView>
  );
}
