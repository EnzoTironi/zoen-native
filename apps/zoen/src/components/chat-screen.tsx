import { FlashList } from '@shopify/flash-list';
import { Pressable, Text, View } from 'react-native';
import { launchConversationAssistant } from '../assistant/launch';
import { ChatHeader } from '../components/chat-header';
import { ConversationRail, ConversationRailSpacer } from '../components/conversation-rail';
import { ConversationTimelineMessage as ConversationMessage } from '../components/conversation-timeline-message';
import { ConversationContainer, ConversationViewport } from '../components/conversation-viewport';
import { MessageComposer } from '../components/message-composer';
import { EmptyState, Loading } from '../components/primitives';
import { conversationQueryFits } from '../core/conversation-search';
import { messageCard } from '../core/message-presentation';
import { t } from '../i18n/index.ts';
import { MiniappRail } from '../miniapps/rail';
import type { useChatController } from './use-chat-controller';

/** Stateless chat composition; the controller retains route, timeline, search and draft ownership. */
export function renderChatScreen(context: ReturnType<typeof useChatController>) {
  const {
    chat,
    id,
    activityLabel,
    nativeScrollTag,
    bindNativeTimeline,
    searchState,
    cancelInitialEnd,
    store,
    setScrollTarget,
    search,
    input,
    initialEnd,
    searchReveal,
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
    searchRevealBusy,
    searchRevealError,
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
    selectedSearchId,
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
    <ConversationContainer style={{ flex: 1, backgroundColor: 'transparent' }}>
      <ChatHeader
        id={id}
        scrollViewTag={nativeScrollTag}
        navigationMetrics={navigationMetrics}
        title={chat.title}
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
          searchReveal.onScroll(event.nativeEvent);
        }}
        onNativeLayout={(height) => {
          bindNativeTimeline();
          initialEnd.onViewportLayout(height);
          searchReveal.onLayout(height);
        }}
        onNativeInset={(inset) => {
          searchReveal.onInset(inset);
          onInsetRequested(inset);
        }}
        onComposerLayout={onComposerLayout}
        composer={
          <>
            <MessageComposer
              context={
                !searchState.active && (normalDraft.replyTo || editing)
                  ? {
                      entry: editing ?? reply ?? undefined,
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
              allowEmptyEdit={Boolean(editing && messageCard(editing)?.kind_id === 'file')}
              replyTo={normalDraft.replyTo}
              draft={
                searchState.active || editing
                  ? undefined
                  : {
                      ready: normalDraft.ready,
                      error: Boolean(normalDraft.error),
                      rejected: normalDraft.rejected,
                      onRetry: normalDraft.retry,
                    }
              }
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
                      state: searchState,
                      status: [
                        !conversationQueryFits(searchState.query.trim())
                          ? t('chat.searchQueryTooLong')
                          : searchState.loading || searchRevealBusy
                            ? t('chat.searchingMessages')
                            : searchState.error ||
                              searchRevealError ||
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
                        const current = search.getSnapshot();
                        if (
                          search.isCurrent() &&
                          current.completed &&
                          !current.loading &&
                          !current.error &&
                          !current.hits.length &&
                          !current.cursor &&
                          current.query.trim() &&
                          conversationQueryFits(current.query.trim())
                        )
                          launchConversationAssistant(id, current.query.trim(), store);
                      },
                    }
                  : undefined
              }
              onMediaSent={normalDraft.mediaAccepted}
            />
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
            <FlashList
              style={{ flex: 1, marginTop: timelineTopInset, overflow: 'hidden' }}
              key={initialEnd.scope}
              ref={list}
              testID="conversation-timeline"
              renderScrollComponent={ScrollComponent}
              onLoad={() => {
                bindNativeTimeline();
                setLoadedSpace(timelineKey);
                initialEnd.onLoad();
              }}
              onContentSizeChange={(width, height) => {
                initialEnd.onContentSizeChange(width, height);
                searchReveal.onContentSize(width, height);
              }}
              onCommitLayoutEffect={() => {
                initialEnd.onCommitLayoutEffect();
                searchReveal.onCommit(visibleEntries);
                // Preserve the native keyboard scroller when search changes the
                // timeline, and address its new indices only after SDK commit.
                if (loadedSpace !== timelineKey) setLoadedSpace(timelineKey);
              }}
              onScrollBeginDrag={() => {
                cancelInitialEnd();
                searchReveal.cancel();
              }}
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
                startRenderingFromBottom: true,
                autoscrollToBottomThreshold: 0.12,
              }}
              onStartReached={() => {
                if (!searchThread && page.has_more)
                  void store.loadTimeline(id, true).catch(() => {});
              }}
              onStartReachedThreshold={0.1}
              ListEmptyComponent={
                <EmptyState
                  title={t('chat.theStartOfSomethingGood')}
                  detail={t('chat.writeTheFirstMessageInThisChat')}
                />
              }
              renderItem={({ item: { entry, activity, album }, index }) => (
                <ConversationMessage
                  spaceId={id}
                  entry={entry}
                  activity={activity}
                  album={album}
                  previous={rows[index - 1]?.entry}
                  next={rows[index + 1]?.entry}
                  groupChat={chat.kind !== 'Direct'}
                  item={items.find((item) => item.id === messageCard(entry)?.item_id)}
                  highlighted={entry.id === selectedSearchId}
                  revealQuery={searchState.query}
                  onRevealLayout={entry.id === selectedSearchId ? onSearchContentLayout : undefined}
                  disabled={searchState.active}
                  reactionPending={actions.busy}
                  onReply={actions.reply}
                  onThread={actions.thread}
                  onAuthor={actions.profile}
                  onMenu={actions.open}
                  onReact={(message, emoji) => {
                    void actions.react(message, emoji);
                  }}
                  onQuote={(messageId) => {
                    void reveal(messageId)
                      .then((target) => setScrollTarget(target))
                      .catch(
                        () =>
                          void inform(
                            t('chat.theMessageCouldNotBeOpened'),
                            t('chat.tryAgainInAMoment'),
                          ),
                      );
                  }}
                />
              )}
            />
          )
        }
      </ConversationViewport>
      {actions.panel}
    </ConversationContainer>
  );
}
