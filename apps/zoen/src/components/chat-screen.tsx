import { ConversationReturnButton } from './conversation-return-button';
import { ConversationHashtagProvider } from './conversation-hashtag-provider';
import { openChatQuote } from './open-chat-quote';
import { router } from 'expo-router';
import { QuoteMotionView } from './quote-layout-context';
import { QuoteLayoutTransition } from './quote-layout-transition';
import { FlashList } from '@shopify/flash-list';
import { Pressable, Text, View } from 'react-native';
import { launchConversationAssistant } from '../assistant/launch';
import { MessageSelectionToolbar } from './message-selection-toolbar';
import { ChatHeader } from '../components/chat-header';
import { ChatDateNavigation } from './chat-date-navigation';
import { ConversationRail, ConversationRailSpacer } from '../components/conversation-rail';
import { ConversationTimelineMessage as ConversationMessage } from '../components/conversation-timeline-message';
import { ConversationContainer, ConversationViewport } from '../components/conversation-viewport';
import { MessageComposer } from '../components/message-composer';
import { EmptyState, Loading } from '../components/primitives';
import { conversationQueryFits } from '../core/conversation-search';
import { messageCard } from '../core/message-presentation';
import { t } from '../i18n/index.ts';
import { conversationTitle } from '../saved-messages/presentation';
import { MiniappRail } from '../miniapps/rail';
import type { useChatController } from './use-chat-controller';

/** Stateless chat composition; the controller retains route, timeline, search and draft ownership. */
export function renderChatScreen(context: ReturnType<typeof useChatController>) {
  const {
    chat,
    id,
    activityLabel,
    selection,
    nativeScrollTag,
    bindNativeTimeline,
    searchState,
    cancelInitialEnd,
    store,
    setScrollTarget,
    messageNavigation,
    search,
    input,
    initialEnd,
    timelineReveal,
    navigationMetrics,
    onComposerLayout,
    onInsetRequested,
    onSearchContentLayout,
    normalDraft,
    editing,
    reply,
    updateDraft,
    composerText,
    sending,
    send,
    timelineRevealBusy,
    timelineRevealError,
    searchThread,
    headerHeight,
    setRailHeight,
    cards,
    conversationMiniapps,
    home,
    setNoticeHeight,
    colors,
    notice,
    page,
    timelineTopInset,
    list,
    setLoadedSpace,
    timelineKey,
    visibleEntries,
    loadedSpace,
    rows,
    railHeight,
    rail,
    Math,
    noticeHeight,
    items,
    highlightedMessageId,
    actions,
    reveal,
    inform,
  } = context;
  if (!chat)
    return (
      <EmptyState
        title={t('chat.chatUnavailable')}
        detail={t('chat.goBackToYourChatsAndTryOpening')}
      />
    );
  return (
    <ConversationHashtagProvider
      search={search}
      onActivate={() => {
        cancelInitialEnd();
        store.setTyping(id, false);
        input.current?.focus();
      }}
    >
      <ConversationContainer style={{ flex: 1, backgroundColor: 'transparent' }}>
        <ChatHeader
          id={id}
          selection={
            selection.active ? { count: selection.count, onCancel: selection.cancel } : undefined
          }
          scrollViewTag={nativeScrollTag}
          navigationMetrics={navigationMetrics}
          title={conversationTitle(chat)}
          saved={chat.kind === 'saved'}
          counterpart={chat.counterpart}
          activityLabel={activityLabel}
          search={{
            active: searchState.active,
            onToggle: () => {
              cancelInitialEnd();
              store.setTyping(id, false);
              if (searchState.active) {
                setScrollTarget(null);
                search.close();
              } else search.open();
              input.current?.focus();
            },
          }}
        />
        <ConversationViewport
          initialFooterKey={initialEnd.scope}
          onInitialFooterLayout={initialEnd.onFooterLayout}
          onNativeScroll={(event) => {
            initialEnd.onNativeScroll(event.nativeEvent);
            timelineReveal.onScroll(event.nativeEvent);
          }}
          onNativeLayout={(height) => {
            bindNativeTimeline();
            initialEnd.onViewportLayout(height);
            timelineReveal.onLayout(height);
          }}
          onNativeInset={(inset) => {
            timelineReveal.onInset(inset);
            onInsetRequested(inset);
          }}
          onComposerLayout={onComposerLayout}
          navigation={
            searchState.active || selection.active ? undefined : (
              <ConversationReturnButton
                navigation={messageNavigation}
                onError={() => {
                  void inform(t('chat.messageUnavailable'), t('chat.tryAgain'));
                }}
              />
            )
          }
          composer={
            <>
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
                    !searchState.active &&
                    (normalDraft.replyTo || normalDraft.sharedReply || editing)
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
                          shared: editing ? undefined : normalDraft.sharedReply,
                          replyIntent: editing ? undefined : normalDraft.replyIntent,
                          onCancelCaptured: normalDraft.cancelMediaReply,
                          onCancel: () => updateDraft({ type: 'cancel' }),
                        }
                      : undefined
                  }
                  spaceId={id}
                  suspended={searchState.active}
                  inputRef={input}
                  onFocus={cancelInitialEnd}
                  value={searchState.active ? searchState.query : composerText}
                  sending={sending}
                  editing={Boolean(editing) && !searchState.active}
                  allowEmptyEdit={Boolean(editing?.isCaption)}
                  replyTo={normalDraft.replyTo}
                  draft={searchState.active ? undefined : normalDraft}
                  onChangeText={(text) => {
                    if (searchState.active) {
                      search.setQuery(text);
                      store.setTyping(id, false);
                    } else if (editing) {
                      updateDraft({ type: 'text', value: text });
                      store.setTyping(id, false);
                    } else {
                      updateDraft({ type: 'text', value: text });
                      store.setTyping(id, Boolean(text.trim()));
                    }
                  }}
                  onBlur={() => {
                    store.setTyping(id, false);
                    normalDraft.flush();
                  }}
                  onSend={() => {
                    if (!searchState.active) void send();
                  }}
                  search={
                    searchState.active
                      ? {
                          calendar: (
                            <ChatDateNavigation
                              spaceId={id}
                              onOpen={() => {
                                cancelInitialEnd();
                                input.current?.blur();
                              }}
                              reveal={reveal}
                              onSelect={(target) => {
                                search.close();
                                setScrollTarget(target);
                              }}
                            />
                          ),
                          state: searchState,
                          status: [
                            !conversationQueryFits(searchState.query.trim())
                              ? t('chat.searchQueryTooLong')
                              : searchState.loading || timelineRevealBusy
                                ? t('chat.searchingMessages')
                                : searchState.error ||
                                  timelineRevealError ||
                                  (searchState.completed
                                    ? searchState.hits.length
                                      ? t('chat.searchPosition', {
                                          position: searchState.index + 1,
                                          count: searchState.hits.length,
                                          more: searchState.cursor ? '+' : '',
                                        })
                                      : t('chat.noResults')
                                    : t('chat.searchUsesTheHistoryAvailableOnThisDevice')),
                            searchThread ? t('chat.searchThreadResult') : null,
                          ]
                            .filter(Boolean)
                            .join(' · '),
                          onClose: () => {
                            setScrollTarget(null);
                            search.close();
                            input.current?.focus();
                          },
                          onMove: (direction) => {
                            void search.move(direction);
                          },
                          onSearch: () => {
                            void search.search();
                          },
                          onAsk: () => {
                            const query = search.assistantQuery(searchState);
                            if (query) launchConversationAssistant(id, query, store);
                          },
                        }
                      : undefined
                  }
                  onMediaSent={normalDraft.mediaAccepted}
                />
              )}
            </>
          }
          overlay={
            <View
              pointerEvents="box-none"
              style={{
                position: 'absolute',
                top: headerHeight,
                left: 0,
                right: 0,
                zIndex: 1,
                backgroundColor: 'transparent',
              }}
            >
              <ConversationRail
                onHeight={setRailHeight}
                height={railHeight}
                motion={rail}
                collapsible={conversationMiniapps.length > 0}
              >
                <MiniappRail
                  key={`${cards.ownerKey}:${id}`}
                  onReorder={cards.reorder}
                  onEditingChange={rail.onEditingChange}
                  items={conversationMiniapps}
                  paddingVertical={6}
                  error={home.error}
                  onRemove={(item) => cards.setPinned(item.id, false)}
                  onTogglePin={(item) =>
                    home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id)
                  }
                  isPinned={home.isPinned}
                />
              </ConversationRail>
              <View onLayout={({ nativeEvent }) => setNoticeHeight(nativeEvent.layout.height)}>
                {cards.error ? (
                  <Pressable
                    accessibilityRole={cards.retryable ? 'button' : 'text'}
                    accessibilityLabel={
                      cards.retryable ? t('conversationCards.tryAgain') : cards.error
                    }
                    disabled={!cards.retryable}
                    onPress={cards.retry}
                    style={{ padding: 12 }}
                  >
                    <Text accessibilityRole="alert" style={{ color: colors.danger }}>
                      {cards.error}
                    </Text>
                  </Pressable>
                ) : null}
                {notice ? (
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel={t('chat.dismissNotice', { notice })}
                    onPress={() => store.dismissNotice()}
                    style={{ padding: 12, backgroundColor: colors.muted }}
                  >
                    <Text accessibilityRole="alert" style={{ color: colors.danger, fontSize: 13 }}>
                      {notice}
                    </Text>
                  </Pressable>
                ) : null}
              </View>
            </View>
          }
        >
          {(composerSpacer, ScrollComponent) =>
            !page ? (
              <Loading />
            ) : (
              <QuoteLayoutTransition key={initialEnd.scope} list={list} onBegin={cancelInitialEnd}>
                {(quoteTransitionActive, quoteCommit, quoteCancel, quoteReading) => (
                  <FlashList
                    style={{ flex: 1, marginTop: timelineTopInset, overflow: 'hidden' }}
                    key={initialEnd.scope}
                    ref={list}
                    testID="conversation-timeline"
                    renderScrollComponent={ScrollComponent}
                    CellRendererComponent={
                      process.env.EXPO_OS === 'ios' ? QuoteMotionView : undefined
                    }
                    onLoad={() => {
                      bindNativeTimeline();
                      setLoadedSpace(timelineKey);
                      initialEnd.onLoad();
                    }}
                    onContentSizeChange={(width, height) => {
                      initialEnd.onContentSizeChange(width, height);
                      timelineReveal.onContentSize(width, height);
                    }}
                    onCommitLayoutEffect={() => {
                      quoteCommit();
                      initialEnd.onCommitLayoutEffect();
                      timelineReveal.onCommit(visibleEntries);
                      // Preserve the native keyboard scroller when search changes the
                      // timeline, and address its new indices only after SDK commit.
                      if (loadedSpace !== timelineKey) setLoadedSpace(timelineKey);
                    }}
                    onScrollBeginDrag={() => {
                      quoteCancel();
                      cancelInitialEnd();
                      timelineReveal.cancel();
                      setScrollTarget(null);
                    }}
                    extraData={selection.ids}
                    data={rows}
                    keyExtractor={(row) => row.key}
                    keyboardDismissMode="interactive"
                    keyboardShouldPersistTaps="handled"
                    contentInsetAdjustmentBehavior="never"
                    ListHeaderComponent={<ConversationRailSpacer motion={rail} />}
                    ListFooterComponent={composerSpacer}
                    contentContainerStyle={{
                      paddingHorizontal: 16,
                      paddingTop: Math.max(0, headerHeight - timelineTopInset) + noticeHeight + 8,
                      paddingBottom: 12,
                    }}
                    maintainVisibleContentPosition={{
                      disabled: quoteTransitionActive,
                      startRenderingFromBottom: true,
                      autoscrollToBottomThreshold:
                        quoteReading || highlightedMessageId || searchState.active
                          ? undefined
                          : 0.12,
                    }}
                    onStartReached={() => {
                      if (!searchThread && page.has_more)
                        void store.loadTimeline(id, true).catch(() => {});
                    }}
                    onStartReachedThreshold={0.1}
                    ListEmptyComponent={
                      <EmptyState
                        title={
                          chat.kind === 'saved'
                            ? t('settings.savedMessages')
                            : t('chat.theStartOfSomethingGood')
                        }
                        detail={t(
                          chat.kind === 'saved'
                            ? 'settings.savedMessagesEmpty'
                            : 'chat.writeTheFirstMessageInThisChat',
                        )}
                      />
                    }
                    renderItem={({ item: { entry, activity, album }, index }) => (
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
                        spaceId={id}
                        entry={entry}
                        activity={activity}
                        album={album}
                        previous={rows[index - 1]?.entry}
                        next={rows[index + 1]?.entry}
                        groupChat={chat.kind === 'Group' || chat.kind === 'Community'}
                        item={items.find((item) => item.id === messageCard(entry)?.item_id)}
                        highlighted={entry.id === highlightedMessageId}
                        revealQuery={searchState.active ? searchState.query : ''}
                        onRevealLayout={
                          entry.id === highlightedMessageId ? onSearchContentLayout : undefined
                        }
                        disabled={searchState.active || selection.active}
                        reactionPending={actions.busy}
                        onReply={actions.reply}
                        onThread={actions.thread}
                        onAuthor={actions.profile}
                        onMenu={actions.open}
                        onReact={(message, emoji) => {
                          void actions.react(message, emoji);
                        }}
                        onQuote={(messageId, sourceSpace, originId) =>
                          openChatQuote(
                            context,
                            router,
                            messageId,
                            sourceSpace,
                            originId ?? entry.id,
                          )
                        }
                      />
                    )}
                  />
                )}
              </QuoteLayoutTransition>
            )
          }
        </ConversationViewport>
        {actions.panel}
      </ConversationContainer>
    </ConversationHashtagProvider>
  );
}
