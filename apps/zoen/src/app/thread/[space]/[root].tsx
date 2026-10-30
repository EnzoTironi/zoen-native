import { homeSpacesFor } from '../../../home/access';
import { t } from '../../../i18n/index.ts';
import { QuoteLayoutTransition } from '../../../components/quote-layout-transition';
import { QuoteMotionView } from '../../../components/quote-layout-context';
import { useCallback, useMemo, useRef, useState } from 'react';
import { router, Stack, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import { Text, type TextInputInstance } from 'react-native';
import { useCoreQuery } from '../../../core/query';
import { useClient } from '../../../core/provider';
import { useMessageAudioFocus } from '../../../media/message-audio-provider';
import { supportsMediaMosaic } from '../../../media/mosaic-layout';
import { messageText } from '../../../core/presentation';
import { messageCard } from '../../../core/message-presentation';
import {
  projectConversationTimeline,
  type ConversationTimelineRow,
} from '../../../core/conversation-timeline';
import { useMessageDraft } from '../../../core/use-message-draft';
import { EmptyState, Loading, NativeButton } from '../../../components/primitives';
import { useTheme } from '../../../theme';
import { useHome } from '../../../home/provider';
import { MessageComposer } from '../../../components/message-composer';
import { useMessageSelection } from '../../../components/use-message-selection';
import {
  MessageSelectionToolbar,
  MessageSelectionHeader,
  MessageSelectionCancel,
} from '../../../components/message-selection-toolbar';
import {
  ConversationContainer,
  ConversationViewport,
} from '../../../components/conversation-viewport';
import { ConversationTimelineMessage as ConversationMessage } from '../../../components/conversation-timeline-message';
import { useMessageActions } from '../../../components/use-message-actions';

export default function Thread() {
  const { space, root, jump } = useLocalSearchParams<{
    space: string;
    root: string;
    jump?: string;
  }>();
  const { store, snapshot, notice, mode } = useClient();
  const owner = JSON.stringify([
    mode,
    snapshot?.account?.identity_id,
    snapshot?.account?.device_id,
    store.sessionEpoch,
  ]);
  useMessageAudioFocus(space, owner);
  const groupChat =
    snapshot?.conversations.some(
      (chat) => chat.id === space && (chat.kind === 'Group' || chat.kind === 'Community'),
    ) ?? false;
  const headerHeight = useHeaderHeight();
  const { top: statusBarHeight } = useSafeAreaInsets();
  const timelineTopInset = process.env.EXPO_OS === 'ios' ? 0 : statusBarHeight;
  const { allItems: items } = useHome();
  const input = useRef<TextInputInstance>(null);
  const list = useRef<FlashListRef<ConversationTimelineRow>>(null);
  const sending = useRef(false);
  useFocusEffect(useCallback(() => () => store.setTyping(space, false), [space, store]));
  const {
    data: entries,
    error,
    retry,
  } = useCoreQuery({ op: 'thread', space_id: space, root_id: root });
  const selection = useMessageSelection(space, entries ?? [], false, root);
  const rows = useMemo<ConversationTimelineRow[]>(
    () =>
      selection.active
        ? (entries ?? []).map((entry) => ({ key: entry.id, entry }))
        : projectConversationTimeline(entries ?? [], items, space, root, supportsMediaMosaic),
    [entries, items, space, root, selection.active],
  );
  const { colors } = useTheme();
  const jumpIndex = jump
    ? rows.findIndex(
        (row) => row.entry.id === jump || row.album?.entries.some((entry) => entry.id === jump),
      )
    : -1;
  const normalDraft = useMessageDraft(space, root);
  const { messageDraft, updateDraft } = normalDraft;
  const { text: draft, reply, editing, editText: editDraft } = messageDraft;
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const actions = useMessageActions({
    spaceId: space,
    threadRootId: root,
    entries: entries ?? [],
    disabled: selection.active,
    onSelect: selection.toggle,
    onReply: (entry, selection) => {
      updateDraft({ type: 'reply', entry, selection });
      input.current?.focus();
    },
    onEdit: (entry) => {
      updateDraft({ type: 'edit', entry, text: messageText(entry) });
      input.current?.focus();
    },
    onThread: () => input.current?.focus(),
  });
  async function send() {
    if (selection.active) return;
    const attempt = normalDraft.capture();
    if (!attempt) return;
    const text = attempt.text;
    if ((!text.trim() && !(editing && messageCard(editing)?.kind_id === 'file')) || sending.current)
      return;
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
      else await normalDraft.send();
      updateDraft({ type: 'accepted', attempt });
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : t('chat.theReplyCouldNotBeSent'));
    } finally {
      sending.current = false;
      setBusy(false);
    }
  }
  return (
    <ConversationContainer style={{ flex: 1, backgroundColor: 'transparent' }}>
      <Stack.Screen
        options={{
          title: selection.active ? String(selection.count) : t('chat.replies'),
          headerBackVisible: !selection.active,
          headerTitle: selection.active
            ? () => <MessageSelectionHeader count={selection.count} onCancel={selection.cancel} />
            : undefined,
          headerLeft: selection.active && process.env.EXPO_OS !== 'ios' ? () => null : undefined,
          headerRight:
            selection.active && process.env.EXPO_OS !== 'ios'
              ? () => <MessageSelectionCancel onCancel={selection.cancel} />
              : undefined,
          unstable_headerLeftItems:
            selection.active && process.env.EXPO_OS === 'ios' ? () => [] : undefined,
          unstable_headerRightItems:
            selection.active && process.env.EXPO_OS === 'ios'
              ? () => [{ type: 'button', label: t('chat.cancel'), onPress: selection.cancel }]
              : undefined,
          scrollEdgeEffects: { top: 'automatic', bottom: 'hidden' },
        }}
      />
      <ConversationViewport
        composer={
          <>
            {failure || error || notice || (jump && entries && jumpIndex < 0) ? (
              <Text
                accessibilityRole="alert"
                selectable
                style={{ color: colors.danger, padding: 16 }}
              >
                {failure ?? error ?? notice ?? t('chat.messageUnavailable')}
              </Text>
            ) : null}
            {selection.active ? (
              <MessageSelectionToolbar
                count={selection.count}
                onCopy={selection.copy}
                onForward={selection.forward}
                canCopy={selection.canCopy}
                canForward={selection.canForward}
              />
            ) : (
              <MessageComposer
                context={
                  normalDraft.replyTo || editing
                    ? {
                        entry: editing ?? reply ?? undefined,
                        quote: editing ? undefined : normalDraft.replyQuote,
                        onCancel: () => updateDraft({ type: 'cancel' }),
                      }
                    : undefined
                }
                threadRootId={root}
                spaceId={space}
                inputRef={input}
                replyTo={normalDraft.replyTo ?? root}
                thread
                editing={Boolean(editing)}
                allowEmptyEdit={Boolean(editing && messageCard(editing)?.kind_id === 'file')}
                value={editing ? editDraft : draft}
                draft={
                  editing
                    ? undefined
                    : {
                        ready: normalDraft.ready,
                        error: Boolean(normalDraft.error),
                        rejected: normalDraft.rejected,
                        onRetry: normalDraft.retry,
                      }
                }
                sending={busy}
                onChangeText={(text) => {
                  updateDraft({ type: 'text', value: text });
                  store.setTyping(space, !editing && Boolean(text.trim()));
                }}
                onBlur={() => {
                  store.setTyping(space, false);
                  normalDraft.flush();
                }}
                onSend={() => {
                  void send();
                }}
                onMediaSent={normalDraft.mediaAccepted}
              />
            )}
          </>
        }
      >
        {(composerSpacer, ScrollComponent) =>
          error && !entries ? (
            <EmptyState title={t('chat.theRepliesCouldNotBeOpened')} detail={error}>
              <NativeButton label={t('chat.tryAgain2')} onPress={retry} />
            </EmptyState>
          ) : !entries ? (
            <Loading />
          ) : (
            <QuoteLayoutTransition key={`${owner}:${space}:${root}:${jump ?? ''}`} list={list}>
              {(active, commit, cancel, reading) => (
                <FlashList
                  style={{ flex: 1, marginTop: timelineTopInset, overflow: 'hidden' }}
                  ref={list}
                  testID="conversation-timeline"
                  renderScrollComponent={ScrollComponent}
                  CellRendererComponent={
                    process.env.EXPO_OS === 'ios' ? QuoteMotionView : undefined
                  }
                  onCommitLayoutEffect={commit}
                  onScrollBeginDrag={cancel}
                  extraData={selection.ids}
                  data={rows}
                  initialScrollIndex={jumpIndex >= 0 ? jumpIndex : undefined}
                  keyExtractor={(row) => row.key}
                  keyboardDismissMode="interactive"
                  keyboardShouldPersistTaps="handled"
                  contentInsetAdjustmentBehavior="never"
                  ListFooterComponent={composerSpacer}
                  contentContainerStyle={{
                    paddingHorizontal: 16,
                    paddingTop: Math.max(0, headerHeight - timelineTopInset) + 8,
                    paddingBottom: 12,
                  }}
                  maintainVisibleContentPosition={{
                    disabled: active,
                    startRenderingFromBottom: false,
                    autoscrollToBottomThreshold: jump || reading ? undefined : 0.15,
                  }}
                  renderItem={({ item: { entry, activity, album }, index }) => (
                    <QuoteMotionView>
                      <ConversationMessage
                        selection={
                          selection.active
                            ? {
                                selected: selection.ids.has(entry.id),
                                disabled: selection.limitReached && !selection.ids.has(entry.id),
                                onToggle: selection.toggle,
                              }
                            : undefined
                        }
                        disabled={selection.active}
                        spaceId={space}
                        entry={entry}
                        highlighted={Boolean(
                          jump &&
                          (entry.id === jump || album?.entries.some((child) => child.id === jump)),
                        )}
                        activity={activity}
                        album={album}
                        previous={index > 1 ? rows[index - 1]?.entry : undefined}
                        next={index > 0 ? rows[index + 1]?.entry : undefined}
                        groupChat={groupChat}
                        item={items.find((item) => item.id === messageCard(entry)?.item_id)}
                        reactionPending={actions.busy}
                        onReply={actions.reply}
                        onThread={actions.thread}
                        onAuthor={actions.profile}
                        onMenu={actions.open}
                        onReact={(message, emoji) => {
                          void actions.react(message, emoji);
                        }}
                        onQuote={(messageId, sourceSpace) => {
                          if (sourceSpace) {
                            if (homeSpacesFor(store.getSnapshot()).has(sourceSpace))
                              router.push({
                                pathname: '/chat/[id]',
                                params: { id: sourceSpace, jump: messageId },
                              });
                            return;
                          }
                          const target = rows.findIndex(
                            (row) =>
                              row.entry.id === messageId ||
                              row.album?.entries.some((entry) => entry.id === messageId),
                          );
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
                    </QuoteMotionView>
                  )}
                />
              )}
            </QuoteLayoutTransition>
          )
        }
      </ConversationViewport>
      {actions.panel}
    </ConversationContainer>
  );
}
