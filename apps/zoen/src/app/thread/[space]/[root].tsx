import { ConversationReturnButton } from '../../../components/conversation-return-button';
import { sendChatMessage } from '../../../core/send-chat-message';
import { NavigationContainer } from '../../../components/navigation-container';
import { useThreadMessageNavigation } from '../../../components/use-thread-message-navigation';
import { ThreadHashtagProvider } from '../../../components/thread-hashtag-provider';
import { t } from '../../../i18n/index.ts';
import { QuoteLayoutTransition } from '../../../components/quote-layout-transition';
import { QuoteMotionView } from '../../../components/quote-layout-context';
import { useCallback, useMemo, useRef, useState } from 'react';
import { Stack, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import { Text, type TextInputInstance } from 'react-native';
import { useCoreQuery } from '../../../core/query';
import { useClient } from '../../../core/provider';
import { useMessageAudioFocus } from '../../../media/message-audio-provider';
import { messageText } from '../../../core/presentation';
import { messageCard } from '../../../core/message-presentation';
import type { ConversationTimelineRow } from '../../../core/conversation-timeline';
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
  useFocusEffect(useCallback(() => () => store.setTyping(space, false), [space, store]));
  const {
    data: entries,
    error,
    retry,
  } = useCoreQuery({ op: 'thread', space_id: space, root_id: root });
  const selection = useMessageSelection(space, entries ?? [], false, root);
  const { colors, presentation: p } = useTheme();
  const normalDraft = useMessageDraft(space, root);
  const { messageDraft, updateDraft } = normalDraft;
  const { text: draft, reply, editing, editText: editDraft } = messageDraft;
  const navigation = useThreadMessageNavigation({
    space,
    root,
    jump,
    entries,
    items,
    list,
    disabled: selection.active,
    headerHeight,
    timelineTopInset,
    reply: Boolean(reply),
    editing: Boolean(editing),
  });
  const { rows } = navigation;
  const jumpIndex = jump
    ? rows.findIndex(
        (row) => row.entry.id === jump || row.album?.entries.some((entry) => entry.id === jump),
      )
    : -1;
  const sendScope = useMemo(
    () => ({ owner, space, root, relay: snapshot?.account?.relay_url }),
    [owner, space, root, snapshot?.account?.relay_url],
  );
  const sending = useRef<{ scope: typeof sendScope } | null>(null);
  const [pendingSend, setPendingSend] = useState<typeof sendScope | null>(null);
  const busy = pendingSend === sendScope;
  const [sendFailure, setFailure] = useState<{ scope: typeof sendScope; message: string } | null>(
    null,
  );
  const failure = sendFailure?.scope === sendScope ? sendFailure.message : null;
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
  function send() {
    setFailure(null);
    return sendChatMessage({
      draft: normalDraft,
      scope: sendScope,
      blocked: selection.active,
      allowEmptyCaption: editing?.isCaption === true,
      flight: sending,
      setPending: setPendingSend,
      typing: () => store.setTyping(space, false),
      inform: async (_title, message) => {
        setFailure({ scope: sendScope, message });
      },
    });
  }
  return (
    <ThreadHashtagProvider space={space}>
      <ConversationContainer style={{ flex: 1, backgroundColor: 'transparent' }}>
        <Stack.Screen
          options={{
            title: selection.active ? String(selection.count) : t('chat.replies'),
            headerBackVisible: !selection.active,
            headerTitle:
              process.env.EXPO_OS === 'ios'
                ? () => (
                    <NavigationContainer
                      scrollViewTag={navigation.scrollViewTag}
                      metrics={navigation.metrics}
                    >
                      {selection.active ? (
                        <MessageSelectionHeader
                          count={selection.count}
                          onCancel={selection.cancel}
                        />
                      ) : (
                        <Text
                          numberOfLines={1}
                          style={{
                            color: p.navigation.text,
                            fontSize: 17,
                            fontWeight: '600',
                            textAlign: 'center',
                          }}
                        >
                          {t('chat.replies')}
                        </Text>
                      )}
                    </NavigationContainer>
                  )
                : selection.active
                  ? () => (
                      <MessageSelectionHeader count={selection.count} onCancel={selection.cancel} />
                    )
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
          onNativeScroll={(event) => navigation.onScroll(event.nativeEvent)}
          onNativeLayout={navigation.onLayout}
          onNativeInset={navigation.onInset}
          onComposerLayout={navigation.onComposerLayout}
          navigation={
            selection.active ? undefined : (
              <ConversationReturnButton
                navigation={navigation.returnNavigation}
                onError={(reason) => {
                  setFailure({
                    scope: sendScope,
                    message:
                      reason instanceof Error ? reason.message : t('chat.messageUnavailable'),
                  });
                }}
              />
            )
          }
          composer={
            <>
              {failure ||
              error ||
              navigation.error ||
              notice ||
              (jump && entries && jumpIndex < 0) ? (
                <Text
                  accessibilityRole="alert"
                  selectable
                  style={{ color: colors.danger, padding: 16 }}
                >
                  {failure ?? error ?? navigation.error ?? notice ?? t('chat.messageUnavailable')}
                </Text>
              ) : null}
              {selection.active ? (
                <MessageSelectionToolbar
                  count={selection.count}
                  onCopy={selection.copy}
                  onForward={selection.forward}
                  canCopy={selection.canCopy}
                  canForward={selection.canForward}
                  canDelete={selection.canDelete}
                  onDelete={selection.remove}
                  pending={selection.pending}
                />
              ) : (
                <MessageComposer
                  context={
                    normalDraft.replyTo || editing
                      ? {
                          entry: editing ? undefined : (reply ?? undefined),
                          edit: editing
                            ? {
                                id: editing.id,
                                revision: editing.text_revision,
                                text: normalDraft.messageDraft.editBaseText,
                              }
                            : undefined,
                          quote: editing ? undefined : normalDraft.replyQuote,
                          replyIntent: editing ? undefined : normalDraft.replyIntent,
                          onCancelCaptured: normalDraft.cancelMediaReply,
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
                  allowEmptyEdit={Boolean(editing?.isCaption)}
                  value={editing ? editDraft : draft}
                  draft={normalDraft}
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
                    onLoad={navigation.onLoad}
                    onContentSizeChange={navigation.onContentSize}
                    onCommitLayoutEffect={() => {
                      commit();
                      navigation.onCommit();
                    }}
                    onScrollBeginDrag={() => {
                      cancel();
                      navigation.cancel();
                    }}
                    extraData={selection.ids}
                    data={rows}
                    initialScrollIndex={
                      process.env.EXPO_OS !== 'ios' && jumpIndex >= 0 ? jumpIndex : undefined
                    }
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
                      autoscrollToBottomThreshold:
                        jump || navigation.target || reading ? undefined : 0.15,
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
                            (process.env.EXPO_OS === 'ios' ? navigation.target : jump) &&
                            (entry.id ===
                              (process.env.EXPO_OS === 'ios' ? navigation.target : jump) ||
                              album?.entries.some(
                                (child) =>
                                  child.id ===
                                  (process.env.EXPO_OS === 'ios' ? navigation.target : jump),
                              )),
                          )}
                          onRevealLayout={
                            entry.id === navigation.target ? navigation.onContentLayout : undefined
                          }
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
                          onQuote={navigation.quote}
                        />
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
    </ThreadHashtagProvider>
  );
}
