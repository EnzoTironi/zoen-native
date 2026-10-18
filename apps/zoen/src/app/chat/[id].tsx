import { t } from '../../i18n/index.ts';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import {
  Pressable,
  Text,
  View,
  type ScrollViewInstance,
  type TextInputInstance,
} from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { useKeyboardMotion } from '../../components/keyboard-motion';
import { useClient } from '../../core/provider';
import type { TimelineEntry } from '../../core/types';
import type { UniversalHit } from '../../core/generated/UniversalHit';
import { chatActivityLabel } from '../../core/live-activity';
import { messageText } from '../../core/presentation';
import { useTheme } from '../../theme';
import { EmptyState, Loading } from '../../components/primitives';
import { MessageComposer } from '../../components/message-composer';
import {
  ConversationContainer,
  ConversationViewport,
} from '../../components/conversation-viewport';
import { ConversationRail, ConversationRailSpacer } from '../../components/conversation-rail';
import { ConversationMessage } from '../../components/conversation-message';
import { useMessageActions } from '../../components/use-message-actions';
import { messageCard } from '../../core/message-presentation';
import { captureMessageDraft } from '../../core/message-draft';
import { useMessageDraft } from '../../core/use-message-draft';
import { ChatHeader } from '../../components/chat-header';
import { useHome } from '../../home/provider';
import { HomeCardMenu } from '../../home/menus';
import { useConversationCards } from '../../miniapps/conversation-provider';
import { MiniappRail } from '../../miniapps/rail';
import { ConversationSearch, conversationQueryFits } from '../../core/conversation-search';
import { launchConversationAssistant } from '../../assistant/launch';
import { InitialTimelineEnd } from '../../core/initial-timeline-end';
import { ConversationReveal } from '../../core/conversation-reveal';

export default function Chat() {
  const { id, jump } = useLocalSearchParams<{ id: string; jump?: string }>();
  const { store, snapshot, timelines, notice, activity } = useClient();
  const { colors } = useTheme();
  const home = useHome();
  const { items } = home;
  const cards = useConversationCards(id);
  const headerHeight = useHeaderHeight();
  const { top: statusBarHeight } = useSafeAreaInsets();
  const { progress: keyboardProgress } = useKeyboardMotion();
  const conversationMiniapps = cards.visible;
  const { inform } = useDialogs();
  const input = useRef<TextInputInstance>(null);
  const list = useRef<FlashListRef<TimelineEntry>>(null);
  const focused = useRef(false);
  const normalDraft = useMessageDraft(id);
  const { messageDraft, updateDraft } = normalDraft;
  const { text: draft, reply, editing, editText: editDraft } = messageDraft;
  const [sending, setSending] = useState(false);
  const sendingNow = useRef(false);
  const [loadedSpace, setLoadedSpace] = useState<string | null>(null);
  const [scrollTarget, setScrollTarget] = useState<string | null>(null);
  const [railHeight, setRailHeight] = useState(0);
  const [noticeHeight, setNoticeHeight] = useState(0);
  const composerText = editing ? editDraft : draft;
  const chat = snapshot?.conversations.find((conversation) => conversation.id === id);
  const owner = [
    store.getSnapshot().mode,
    snapshot?.account?.identity_id ?? snapshot?.me?.id,
    snapshot?.account?.device_id,
  ].join(':');
  const search = useMemo(
    () =>
      new ConversationSearch({
        current: () => {
          const current = store.getSnapshot();
          return (
            current.phase === 'ready' &&
            [
              current.mode,
              current.snapshot?.account?.identity_id ?? current.snapshot?.me?.id,
              current.snapshot?.account?.device_id,
            ].join(':') === owner &&
            Boolean(current.snapshot?.conversations.some((conversation) => conversation.id === id))
          );
        },
        search: (query, cursor) =>
          store.driver.request({ op: 'search_messages', space_id: id, query, cursor, limit: 30 }),
      }),
    [id, owner, store],
  );
  const searchState = useSyncExternalStore(
    search.subscribe,
    search.getSnapshot,
    search.getSnapshot,
  );
  const [searchRead, setSearchRead] = useState<{
    search: ConversationSearch;
    hit: UniversalHit;
    thread: { root: string; entries: TimelineEntry[] } | null;
    error: string | null;
  } | null>(null);
  const selectedHit = searchState.hits[searchState.index];
  const selectedSearchId = searchState.active ? selectedHit?.ref_id : null;
  const currentRead =
    searchState.active && searchRead?.search === search && searchRead.hit === selectedHit
      ? searchRead
      : null;
  const searchThread = currentRead?.thread ?? null;
  const searchRevealBusy = Boolean(searchState.active && selectedHit && !currentRead);
  const searchRevealError = currentRead?.error;
  const page = useMemo(
    () =>
      searchThread
        ? { entries: searchThread.entries, before: null, has_more: false }
        : timelines[id],
    [searchThread, timelines, id],
  );
  const timelineKey = searchState.active && searchThread ? `${id}:${searchThread.root}` : id;
  const initialEnd = useMemo(
    () =>
      new InitialTimelineEnd(`${id}:${owner}`, undefined, {
        anchorUntilInteraction: process.env.EXPO_OS === 'ios',
      }),
    [id, owner],
  );
  const revealLayout = useRef({
    entries: [] as readonly TimelineEntry[],
    ready: false,
    headerHeight,
    statusBarHeight,
    railHeight,
    noticeHeight,
  });
  const searchReveal = useMemo(() => new ConversationReveal(`${id}:${owner}`), [id, owner]);
  useLayoutEffect(() => {
    searchReveal.connect({
      current: (target) => {
        const current = search.getSnapshot();
        return (
          focused.current &&
          search.isCurrent() &&
          current.active &&
          current.hits[current.index]?.ref_id === target
        );
      },
      item: (target) => {
        const current = revealLayout.current;
        const native = list.current;
        if (!current.ready || !native) return null;
        const index = current.entries.findIndex((entry) => entry.id === target);
        if (index < 0) return null;
        const row = native.getLayout(index);
        if (!row) return null;
        return {
          y: row.y,
          height: row.height,
          firstItemOffset: native.getFirstItemOffset(),
          childHeight: native.getChildContainerDimensions().height,
          windowHeight: native.getWindowSize().height,
          bottomAligned: true,
          topObstruction:
            current.headerHeight -
            current.statusBarHeight +
            current.railHeight * (1 - Math.min(1, Math.max(0, keyboardProgress.value))) +
            current.noticeHeight +
            8,
        };
      },
      frame: (work) => {
        const frame = requestAnimationFrame(work);
        return () => cancelAnimationFrame(frame);
      },
      // Unlike scrollToIndex, this public API leaves clamping to our measured
      // native range, which includes the keyboard and composer contentInset.
      scroll: (offset) => list.current?.scrollToOffset({ offset, animated: true }),
    });
  }, [searchReveal, search, keyboardProgress]);
  useLayoutEffect(() => {
    revealLayout.current = {
      entries: page?.entries ?? [],
      ready:
        loadedSpace === timelineKey &&
        !cards.loading &&
        (conversationMiniapps.length === 0 || railHeight > 0),
      headerHeight,
      statusBarHeight,
      railHeight,
      noticeHeight,
    };
    searchReveal.onCommit();
  }, [
    page,
    loadedSpace,
    timelineKey,
    cards.loading,
    conversationMiniapps.length,
    headerHeight,
    statusBarHeight,
    railHeight,
    noticeHeight,
    searchReveal,
  ]);
  useLayoutEffect(() => {
    searchReveal.cancel();
  }, [searchReveal, searchState.active, searchState.query, selectedSearchId]);
  useLayoutEffect(() => () => searchReveal.cancel(), [searchReveal]);
  const cancelInitialEnd = initialEnd.cancel;
  useLayoutEffect(() => {
    initialEnd.connect({
      current: () => {
        const current = store.getSnapshot();
        return (
          focused.current &&
          !jump &&
          !reply &&
          !editing &&
          !scrollTarget &&
          !search.getSnapshot().active &&
          current.phase === 'ready' &&
          Boolean(current.snapshot?.conversations.some((conversation) => conversation.id === id)) &&
          [
            current.mode,
            current.snapshot?.account?.identity_id ?? current.snapshot?.me?.id,
            current.snapshot?.account?.device_id,
          ].join(':') === owner
        );
      },
      frame: (work) => {
        const frame = requestAnimationFrame(work);
        return () => cancelAnimationFrame(frame);
      },
      scroll: () => {
        // FlashList 2.3's declaration names the component instead of its instance.
        const native = list.current?.getNativeScrollRef() as unknown as
          | ScrollViewInstance
          | null
          | undefined;
        if (!native) return false;
        native.scrollToEnd({ animated: false });
        return true;
      },
    });
    if (jump || reply || editing || scrollTarget || searchState.active) cancelInitialEnd();
  }, [
    initialEnd,
    id,
    owner,
    jump,
    search,
    store,
    reply,
    editing,
    scrollTarget,
    searchState.active,
    cancelInitialEnd,
  ]);
  useLayoutEffect(() => () => initialEnd.cancel(), [initialEnd]);
  useEffect(() => {
    if (!searchState.active || !searchState.query.trim()) return;
    const timer = setTimeout(() => {
      void search.search();
    }, 200);
    return () => clearTimeout(timer);
  }, [search, searchState.active, searchState.query]);
  useEffect(() => () => search.close(), [search]);
  const activityLabel = chatActivityLabel(snapshot, activity, id);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      initialEnd.focus();
      store.selectSpace(id);
      return () => {
        focused.current = false;
        initialEnd.blur();
        searchReveal.cancel();
        store.setTyping(id, false);
        store.selectSpace(null);
      };
    }, [id, store, initialEnd, searchReveal]),
  );
  const reveal = useCallback(
    async (messageId: string) => {
      cancelInitialEnd();
      const index = await store.revealMessage(id, messageId);
      if (index < 0) throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
      return messageId;
    },
    [id, store, cancelInitialEnd],
  );
  useEffect(() => {
    let active = true;
    if (!searchState.active || !selectedHit) return;
    const captured = searchReveal.capture();
    const current = () =>
      active && focused.current && search.isCurrent() && searchReveal.isCurrent(captured);
    void (async () => {
      const message = await store.driver.request({
        op: 'message',
        space_id: id,
        message_id: selectedHit.ref_id,
      });
      if (!current()) return;
      let thread: { root: string; entries: TimelineEntry[] } | null = null;
      if (message.in_thread) {
        const entries = await store.driver.request({
          op: 'thread',
          space_id: id,
          root_id: message.in_thread,
        });
        if (!current()) return;
        if (!entries.some((entry) => entry.id === selectedHit.ref_id))
          throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
        thread = { root: message.in_thread, entries };
      } else await reveal(selectedHit.ref_id);
      if (current()) {
        setSearchRead({ search, hit: selectedHit, thread, error: null });
        if (process.env.EXPO_OS === 'ios') searchReveal.request(selectedHit.ref_id, captured);
        else setScrollTarget(selectedHit.ref_id);
      }
    })().catch((reason: unknown) => {
      if (current())
        setSearchRead({
          search,
          hit: selectedHit,
          thread: null,
          error: reason instanceof Error ? reason.message : t('chat.thisResultCouldNotBeOpened'),
        });
    });
    return () => {
      active = false;
    };
  }, [id, store, reveal, selectedHit, searchState.active, search, searchReveal]);
  useFocusEffect(
    useCallback(() => {
      if (!jump) return;
      let active = true;
      void reveal(jump)
        .then((target) => {
          if (active) setScrollTarget(target);
        })
        .catch((reason: unknown) => {
          if (active)
            void inform(
              t('chat.messageUnavailable'),
              reason instanceof Error ? reason.message : t('chat.tryAgain'),
            );
        });
      return () => {
        active = false;
      };
    }, [jump, reveal, inform]),
  );
  useEffect(() => {
    if (
      !scrollTarget ||
      loadedSpace !== timelineKey ||
      !page ||
      cards.loading ||
      (conversationMiniapps.length > 0 && railHeight === 0)
    )
      return;
    const index = page.entries.findIndex((entry) => entry.id === scrollTarget);
    if (index < 0) return;
    // Wait until the loaded history has committed before addressing list indices.
    const frame = requestAnimationFrame(() => {
      void list.current
        ?.scrollToIndex({
          index,
          animated: true,
          viewPosition: 0,
          viewOffset: -(
            headerHeight -
            statusBarHeight +
            railHeight * (1 - Math.min(1, Math.max(0, keyboardProgress.value))) +
            noticeHeight +
            8
          ),
        })
        .catch(() => {
          void inform(t('chat.theMessageCouldNotBeOpened'), t('chat.tryAgain'));
        });
      setScrollTarget(null);
    });
    return () => cancelAnimationFrame(frame);
  }, [
    scrollTarget,
    loadedSpace,
    timelineKey,
    id,
    page,
    inform,
    headerHeight,
    statusBarHeight,
    railHeight,
    noticeHeight,
    keyboardProgress,
    conversationMiniapps.length,
    cards.loading,
  ]);
  const last = page?.entries.at(-1)?.id;
  useFocusEffect(
    useCallback(() => {
      if (last) void store.request({ op: 'mark_read', space_id: id }).catch(() => {});
    }, [last, id, store]),
  );

  async function send() {
    if (search.getSnapshot().active) return;
    if (
      (!composerText.trim() && !(editing && messageCard(editing)?.kind_id === 'file')) ||
      sendingNow.current
    )
      return;
    const attempt = captureMessageDraft(messageDraft);
    const text = attempt.text;
    sendingNow.current = true;
    store.setTyping(id, false);
    setSending(true);
    try {
      if (attempt.editingId)
        await store.request({
          op: 'edit_message',
          space_id: id,
          message_id: attempt.editingId,
          text,
        });
      else await normalDraft.send();
      updateDraft({ type: 'accepted', attempt });
    } catch {
      /* Keep the text for a retry. */
    } finally {
      sendingNow.current = false;
      setSending(false);
    }
  }

  const actions = useMessageActions({
    spaceId: id,
    entries: page?.entries ?? [],
    disabled: searchState.active,
    onReply: (entry) => {
      cancelInitialEnd();
      updateDraft({ type: 'reply', entry });
      input.current?.focus();
    },
    onEdit: (entry) => {
      cancelInitialEnd();
      updateDraft({ type: 'edit', entry, text: messageText(entry) });
      input.current?.focus();
    },
  });

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
          initialEnd.onViewportLayout(height);
          searchReveal.onLayout(height);
        }}
        onNativeInset={searchReveal.onInset}
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
                          launchConversationAssistant(id, current.query.trim());
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
            <ConversationRail onHeight={setRailHeight}>
              <MiniappRail
                items={conversationMiniapps}
                paddingVertical={6}
                renderAccessory={
                  process.env.EXPO_OS === 'ios'
                    ? (item) => (
                        <HomeCardMenu
                          name={item.title}
                          testID={`conversation-card-menu-${item.id}`}
                          pinned={home.isPinned(item.id)}
                          onToggle={() => {
                            void (
                              home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id)
                            ).catch(() => {});
                          }}
                          conversationPinned
                          onToggleConversation={() => {
                            void cards.setPinned(item.id, false).catch(() => {});
                          }}
                          onOrganizeConversation={() =>
                            router.push({ pathname: '/conversation-cards/[id]', params: { id } })
                          }
                        />
                      )
                    : undefined
                }
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
              style={{ flex: 1, marginTop: statusBarHeight, overflow: 'hidden' }}
              key={initialEnd.scope}
              ref={list}
              testID="conversation-timeline"
              renderScrollComponent={ScrollComponent}
              onLoad={() => {
                setLoadedSpace(timelineKey);
                initialEnd.onLoad();
              }}
              onContentSizeChange={(width, height) => {
                initialEnd.onContentSizeChange(width, height);
                searchReveal.onContentSize(width, height);
              }}
              onCommitLayoutEffect={() => {
                initialEnd.onCommitLayoutEffect();
                searchReveal.onCommit();
                // Preserve the native keyboard scroller when search changes the
                // timeline, and address its new indices only after SDK commit.
                if (loadedSpace !== timelineKey) setLoadedSpace(timelineKey);
              }}
              onScrollBeginDrag={() => {
                cancelInitialEnd();
                searchReveal.cancel();
              }}
              data={page.entries}
              keyExtractor={(entry) => entry.id}
              keyboardDismissMode="interactive"
              keyboardShouldPersistTaps="handled"
              contentInsetAdjustmentBehavior="never"
              ListHeaderComponent={<ConversationRailSpacer height={railHeight} />}
              ListFooterComponent={composerSpacer}
              contentContainerStyle={{
                paddingHorizontal: 16,
                paddingTop: Math.max(0, headerHeight - statusBarHeight) + noticeHeight + 8,
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
              renderItem={({ item: entry, index }) => (
                <ConversationMessage
                  spaceId={id}
                  entry={entry}
                  previous={page.entries[index - 1]}
                  next={page.entries[index + 1]}
                  groupChat={chat.kind !== 'Direct'}
                  item={items.find((item) => item.id === messageCard(entry)?.item_id)}
                  highlighted={entry.id === selectedSearchId}
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
