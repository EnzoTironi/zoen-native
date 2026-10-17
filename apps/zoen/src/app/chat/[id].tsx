import { t, formatLocale } from '../../i18n/index.ts';
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import { Host, BottomSheet, Column, Button } from '@expo/ui';
import * as Clipboard from 'expo-clipboard';
import * as Haptics from 'expo-haptics';
import {
  KeyboardAvoidingView,
  useReanimatedKeyboardAnimation,
} from 'react-native-keyboard-controller';
import { Pressable, Text, View, type TextInputInstance } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { useClient } from '../../core/provider';
import type { TimelineEntry } from '../../core/types';
import type { UniversalHit } from '../../core/generated/UniversalHit';
import { chatActivityLabel } from '../../core/live-activity';
import { messageText } from '../../core/presentation';
import { useTheme } from '../../theme';
import { EmptyState, IconButton, Loading } from '../../components/primitives';
import { MessageComposer } from '../../components/message-composer';
import { ConversationViewport } from '../../components/conversation-viewport';
import { ConversationRail, ConversationRailSpacer } from '../../components/conversation-rail';
import { MessageMedia } from '../../media/MediaAttachment';
import { ChatHeader } from '../../components/chat-header';
import { MessageReceipt, messageDeliveryLabel } from '../../components/message-receipt';
import { useHome } from '../../home/provider';
import { MiniappRail } from '../../miniapps/rail';
import { ConversationSearch, conversationQueryFits } from '../../core/conversation-search';
import { launchConversationAssistant } from '../../assistant/launch';

export default function Chat() {
  const { id, jump } = useLocalSearchParams<{ id: string; jump?: string }>();
  const { store, snapshot, timelines, notice, activity } = useClient();
  const { colors, scheme } = useTheme();
  const { items } = useHome();
  const headerHeight = useHeaderHeight();
  const { progress: keyboardProgress } = useReanimatedKeyboardAnimation();
  const conversationMiniapps = items.filter((item) => item.space_id === id);
  const { confirm, inform } = useDialogs();
  const input = useRef<TextInputInstance>(null);
  const list = useRef<FlashListRef<TimelineEntry>>(null);
  const [draft, setDraft] = useState('');
  const [reply, setReply] = useState<TimelineEntry | null>(null);
  const [selected, setSelected] = useState<TimelineEntry | null>(null);
  const [sending, setSending] = useState(false);
  const [editing, setEditing] = useState<TimelineEntry | null>(null);
  const [editDraft, setEditDraft] = useState('');
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
      store.selectSpace(id);
      return () => {
        store.setTyping(id, false);
        store.selectSpace(null);
      };
    }, [id, store]),
  );
  const reveal = useCallback(
    async (messageId: string) => {
      const index = await store.revealMessage(id, messageId);
      if (index < 0) throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
      return messageId;
    },
    [id, store],
  );
  useEffect(() => {
    let active = true;
    if (!searchState.active || !selectedHit) return;
    void (async () => {
      const message = await store.driver.request({
        op: 'message',
        space_id: id,
        message_id: selectedHit.ref_id,
      });
      if (!active || !search.isCurrent()) return;
      let thread: { root: string; entries: TimelineEntry[] } | null = null;
      if (message.in_thread) {
        const entries = await store.driver.request({
          op: 'thread',
          space_id: id,
          root_id: message.in_thread,
        });
        if (!active || !search.isCurrent()) return;
        if (!entries.some((entry) => entry.id === selectedHit.ref_id))
          throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
        thread = { root: message.in_thread, entries };
      } else await reveal(selectedHit.ref_id);
      if (active && search.isCurrent()) {
        setSearchRead({ search, hit: selectedHit, thread, error: null });
        setScrollTarget(selectedHit.ref_id);
      }
    })().catch((reason: unknown) => {
      if (active && search.isCurrent())
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
  }, [id, store, reveal, selectedHit, searchState.active, search]);
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
            headerHeight +
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
    railHeight,
    noticeHeight,
    keyboardProgress,
    conversationMiniapps.length,
  ]);
  const last = page?.entries.at(-1)?.id;
  useFocusEffect(
    useCallback(() => {
      if (last) void store.driver.request({ op: 'mark_read', space_id: id }).catch(() => {});
    }, [last, id, store]),
  );

  async function send() {
    if (search.getSnapshot().active) return;
    if (!composerText.trim() || sending) return;
    const text = composerText;
    store.setTyping(id, false);
    setSending(true);
    try {
      if (editing)
        await store.request({ op: 'edit_message', space_id: id, message_id: editing.id, text });
      else
        await store.request({
          op: 'send_message',
          space_id: id,
          text,
          reply_to: reply?.id ?? null,
          thread: false,
        });
      if (editing) {
        setEditing(null);
        setEditDraft('');
      } else {
        setDraft((value) => (value === text ? '' : value));
        setReply(null);
      }
    } catch {
      /* Keep the text for a retry. */
    } finally {
      setSending(false);
    }
  }

  if (!chat)
    return (
      <EmptyState
        title={t('chat.chatUnavailable')}
        detail={t('chat.goBackToYourChatsAndTryOpening')}
      />
    );
  return (
    <KeyboardAvoidingView
      behavior="padding"
      automaticOffset
      style={{ flex: 1, backgroundColor: 'transparent' }}
    >
      <ChatHeader
        id={id}
        title={chat.title}
        counterpart={chat.counterpart}
        activityLabel={activityLabel}
        search={{
          active: searchState.active,
          onToggle: () => {
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
        composer={
          <>
            {!searchState.active && (reply || editing) ? (
              <View
                style={{
                  marginHorizontal: 16,
                  paddingHorizontal: 14,
                  paddingVertical: 10,
                  borderRadius: 18,
                  backgroundColor: colors.muted,
                  flexDirection: 'row',
                  alignItems: 'center',
                }}
              >
                <View style={{ flex: 1, gap: 4 }}>
                  <Text style={{ color: colors.accent, fontSize: 12, fontWeight: '600' }}>
                    {editing
                      ? t('chat.editMessage')
                      : t('chat.replyingTo', { name: reply?.author.name ?? '' })}
                  </Text>
                  <Text numberOfLines={1} style={{ color: colors.secondary, fontSize: 13 }}>
                    {messageText((editing ?? reply)!)}
                  </Text>
                </View>
                <IconButton
                  label={t('chat.cancel')}
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  onPress={() => {
                    if (editing) {
                      setEditing(null);
                      setEditDraft('');
                    } else setReply(null);
                  }}
                />
              </View>
            ) : null}
            <MessageComposer
              spaceId={id}
              suspended={searchState.active}
              inputRef={input}
              value={searchState.active ? searchState.query : composerText}
              sending={sending}
              editing={Boolean(editing) && !searchState.active}
              replyTo={reply?.id ?? null}
              onChangeText={(text) => {
                if (searchState.active) {
                  search.setQuery(text);
                  store.setTyping(id, false);
                } else if (editing) {
                  setEditDraft(text);
                  store.setTyping(id, false);
                } else {
                  setDraft(text);
                  store.setTyping(id, Boolean(text.trim()));
                }
              }}
              onBlur={() => store.setTyping(id, false)}
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
              onMediaSent={() => setReply(null)}
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
              <MiniappRail items={conversationMiniapps} paddingVertical={6} />
            </ConversationRail>
            <View onLayout={({ nativeEvent }) => setNoticeHeight(nativeEvent.layout.height)}>
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
        {(composerSpacer) =>
          !page ? (
            <Loading />
          ) : (
            <FlashList
              key={timelineKey}
              ref={list}
              onLoad={() => setLoadedSpace(timelineKey)}
              data={page.entries}
              keyExtractor={(entry) => entry.id}
              keyboardDismissMode="interactive"
              keyboardShouldPersistTaps="handled"
              contentInsetAdjustmentBehavior="never"
              ListHeaderComponent={<ConversationRailSpacer height={railHeight} />}
              ListFooterComponent={composerSpacer}
              contentContainerStyle={{
                paddingHorizontal: 16,
                paddingTop: headerHeight + noticeHeight + 8,
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
              renderItem={({ item: entry, index }) => {
                const mine = entry.author.is_me;
                const text = messageText(entry);
                const previous = page.entries[index - 1];
                const next = page.entries[index + 1];
                const grouped =
                  previous?.author.id === entry.author.id &&
                  Number(entry.at_ms) - Number(previous.at_ms) < 180_000;
                const lastInGroup = next?.author.id !== entry.author.id;
                const card = 'Message' in entry.kind ? entry.kind.Message.card : null;
                const system = 'System' in entry.kind || 'Background' in entry.kind;
                if (system)
                  return (
                    <Text
                      selectable
                      style={{
                        fontSize: 12,
                        lineHeight: 19,
                        color: colors.secondary,
                        borderWidth: entry.id === selectedSearchId ? 2 : 0,
                        borderColor: colors.accent,
                        borderRadius: 12,
                        textAlign: 'center',
                        marginVertical: 14,
                        alignSelf: 'center',
                        maxWidth: '95%',
                        paddingHorizontal: 12,
                        paddingVertical: 6,
                      }}
                    >
                      {text}
                    </Text>
                  );
                return (
                  <View
                    style={{
                      alignItems: mine ? 'flex-end' : 'flex-start',
                      marginTop: grouped ? 3 : 16,
                      marginBottom: lastInGroup ? 5 : 0,
                    }}
                  >
                    {!mine && !grouped && chat.kind !== 'Direct' ? (
                      <Text
                        style={{
                          color: colors.secondary,
                          fontWeight: '600',
                          fontSize: 12,
                          marginBottom: 5,
                          paddingHorizontal: 10,
                          paddingVertical: 4,
                        }}
                      >
                        {entry.author.name}
                      </Text>
                    ) : null}
                    <Pressable
                      accessible={!entry.voice && card?.kind_id !== 'file'}
                      onLongPress={() => {
                        if (searchState.active) return;
                        void Haptics.selectionAsync().catch(() => {});
                        setSelected(entry);
                      }}
                      onAccessibilityTap={
                        process.env.EXPO_OS === 'ios' && !searchState.active
                          ? () => setSelected(entry)
                          : undefined
                      }
                      accessibilityRole="button"
                      accessibilityLabel={
                        `${entry.author.name}: ${text}` +
                        (mine ? `, ${messageDeliveryLabel(entry.delivery)}` : '')
                      }
                      accessibilityHint={
                        process.env.EXPO_OS === 'ios'
                          ? t('chat.messageActions', { name: entry.author.name })
                          : t('chat.touchAndHoldToReplyOrSeeActions')
                      }
                      style={{
                        maxWidth: '87%',
                        borderRadius: 23,
                        borderCurve: 'continuous',
                        borderWidth: entry.id === selectedSearchId ? 2 : 0,
                        borderColor: colors.accent,
                        paddingHorizontal: 15,
                        paddingTop: 11,
                        paddingBottom: 8,
                        backgroundColor: mine ? colors.mine : colors.theirs,
                      }}
                    >
                      {entry.reply_to ? (
                        <Pressable
                          accessibilityRole="button"
                          accessibilityLabel={t('chat.goToQuotedMessage')}
                          disabled={searchState.active}
                          onPress={() => {
                            void reveal(entry.reply_to!.id)
                              .then((target) => {
                                setScrollTarget(target);
                              })
                              .catch(
                                () =>
                                  void inform(
                                    t('chat.theMessageCouldNotBeOpened'),
                                    t('chat.tryAgainInAMoment'),
                                  ),
                              );
                          }}
                          style={{
                            borderLeftWidth: 3,
                            borderLeftColor: colors.accent,
                            paddingLeft: 9,
                            paddingVertical: 5,
                            marginBottom: 7,
                            opacity: 0.8,
                          }}
                        >
                          <Text
                            style={{
                              color: mine ? colors.onMine : colors.text,
                              fontWeight: '600',
                              fontSize: 12,
                            }}
                          >
                            {entry.reply_to.author.name}
                          </Text>
                          <Text
                            numberOfLines={2}
                            style={{ color: mine ? colors.onMine : colors.secondary, fontSize: 13 }}
                          >
                            {entry.reply_to.text}
                          </Text>
                        </Pressable>
                      ) : null}
                      {entry.forwarded ? (
                        <Text
                          style={{
                            color: mine ? colors.onMine : colors.secondary,
                            fontSize: 12,
                            fontStyle: 'italic',
                            marginBottom: 5,
                          }}
                        >
                          {t('chat.forwarded')}
                        </Text>
                      ) : null}
                      {!entry.voice && card?.kind_id !== 'file' ? (
                        <Text
                          selectable
                          style={{
                            fontSize: 16,
                            lineHeight: 23,
                            color: mine ? colors.onMine : colors.text,
                            fontStyle: entry.deleted ? 'italic' : 'normal',
                          }}
                        >
                          {entry.deleted ? t('chat.deletedMessage') : text}
                        </Text>
                      ) : null}
                      <MessageMedia entry={entry} />
                      {!searchState.active && (entry.voice || card?.kind_id === 'file') ? (
                        <IconButton
                          label={t('chat.messageActions', { name: entry.author.name })}
                          name={{ ios: 'ellipsis', android: 'more_horiz', web: 'more_horiz' }}
                          onPress={() => setSelected(entry)}
                        />
                      ) : null}
                      {card && card.kind_id !== 'file' ? (
                        <Pressable
                          accessibilityRole="button"
                          accessibilityLabel={t('chat.openNamedItem', { title: card.title })}
                          onPress={() =>
                            router.push({ pathname: '/item/[id]', params: { id: card.item_id } })
                          }
                          style={{
                            padding: 14,
                            marginTop: 10,
                            borderRadius: 16,
                            backgroundColor: colors.surface,
                          }}
                        >
                          <Text
                            style={{
                              color: colors.accent,
                              fontSize: 12,
                              fontWeight: '700',
                              marginBottom: 4,
                            }}
                          >
                            {card.kind_label}
                          </Text>
                          <Text style={{ fontSize: 18, fontWeight: '600', color: colors.text }}>
                            {card.title}
                          </Text>
                          <Text
                            numberOfLines={3}
                            style={{ color: colors.secondary, lineHeight: 20, marginTop: 5 }}
                          >
                            {card.summary}
                          </Text>
                        </Pressable>
                      ) : null}
                      <View
                        style={{
                          flexDirection: 'row',
                          justifyContent: 'flex-end',
                          alignItems: 'center',
                          gap: 5,
                          marginTop: 5,
                        }}
                      >
                        <Text
                          style={{
                            color: mine ? colors.onMine : colors.secondary,
                            fontSize: 11,
                            fontVariant: ['tabular-nums'],
                          }}
                        >
                          {entry.edited
                            ? t('chat.editedAt', {
                                time: new Date(Number(entry.at_ms)).toLocaleTimeString(
                                  formatLocale,
                                  {
                                    hour: '2-digit',
                                    minute: '2-digit',
                                  },
                                ),
                              })
                            : new Date(Number(entry.at_ms)).toLocaleTimeString(formatLocale, {
                                hour: '2-digit',
                                minute: '2-digit',
                              })}
                        </Text>
                        {mine ? <MessageReceipt delivery={entry.delivery} /> : null}
                      </View>
                    </Pressable>
                    {entry.reactions.length ? (
                      <View style={{ flexDirection: 'row', gap: 5, marginTop: 4 }}>
                        {entry.reactions.map((reaction) => (
                          <Pressable
                            key={reaction.emoji}
                            accessibilityRole="button"
                            accessibilityLabel={t('chat.reactionPeople', {
                              emoji: reaction.emoji,
                              count: reaction.people.length,
                            })}
                            onPress={() => {
                              void store
                                .request({
                                  op: 'react',
                                  space_id: id,
                                  message_id: entry.id,
                                  emoji: reaction.mine ? null : reaction.emoji,
                                })
                                .catch(() => {});
                            }}
                            style={{
                              borderRadius: 14,
                              backgroundColor: colors.muted,
                              borderWidth: reaction.mine ? 1 : 0,
                              borderColor: colors.accent,
                              paddingHorizontal: 8,
                              paddingVertical: 4,
                            }}
                          >
                            <Text style={{ fontSize: 13, color: colors.text }}>
                              {reaction.emoji} {reaction.people.length}
                            </Text>
                          </Pressable>
                        ))}
                      </View>
                    ) : null}
                    {entry.thread_replies > 0 ? (
                      <Pressable
                        accessibilityRole="button"
                        accessibilityLabel={t('chat.viewReplies', { count: entry.thread_replies })}
                        onPress={() =>
                          router.push({
                            pathname: '/thread/[space]/[root]',
                            params: { space: id, root: entry.id },
                          })
                        }
                        style={{
                          paddingHorizontal: 12,
                          paddingVertical: 10,
                          marginTop: 4,
                        }}
                      >
                        <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>
                          {t('chat.replyCount', { count: entry.thread_replies })} →
                        </Text>
                      </Pressable>
                    ) : null}
                  </View>
                );
              }}
            />
          )
        }
      </ConversationViewport>
      <Host colorScheme={scheme} seedColor={colors.accent}>
        <BottomSheet isPresented={selected !== null} onDismiss={() => setSelected(null)}>
          <Column spacing={8}>
            <Button
              label={t('chat.reply')}
              variant="text"
              onPress={() => {
                setEditing(null);
                setReply(selected);
                setSelected(null);
                input.current?.focus();
              }}
            />
            <Button
              label={t('chat.openReplies')}
              variant="text"
              onPress={() => {
                if (selected)
                  router.push({
                    pathname: '/thread/[space]/[root]',
                    params: { space: id, root: selected.id },
                  });
                setSelected(null);
              }}
            />
            {selected && !selected.deleted ? (
              <Button
                label={t('chat.forward')}
                variant="text"
                onPress={() => {
                  router.push({
                    pathname: '/forward/[space]/[message]',
                    params: { space: id, message: selected.id },
                  });
                  setSelected(null);
                }}
              />
            ) : null}
            <Button
              label={t('chat.copyText')}
              variant="text"
              onPress={() => {
                if (selected) void Clipboard.setStringAsync(messageText(selected));
                setSelected(null);
              }}
            />
            <Button
              label={t('chat.like')}
              variant="text"
              onPress={() => {
                if (selected)
                  void store
                    .request({ op: 'react', space_id: id, message_id: selected.id, emoji: '❤️' })
                    .catch(() => {});
                setSelected(null);
              }}
            />
            {selected?.author.is_me &&
            !selected.deleted &&
            !selected.voice &&
            'Message' in selected.kind &&
            !selected.kind.Message.card ? (
              <Button
                label={t('chat.edit')}
                variant="text"
                onPress={() => {
                  setEditing(selected);
                  setEditDraft(messageText(selected));
                  setSelected(null);
                  input.current?.focus();
                }}
              />
            ) : null}
            {selected?.author.is_me && !selected.deleted ? (
              <Button
                label={t('chat.deleteForEveryone')}
                variant="text"
                onPress={() => {
                  const entry = selected;
                  setSelected(null);
                  void confirm({
                    title: t('chat.deleteMessage'),
                    message: t('chat.theMessageWillBeRemovedForEveryoneIn'),
                    confirmLabel: t('chat.delete'),
                    destructive: true,
                  }).then((accepted) => {
                    if (accepted)
                      void store
                        .request({ op: 'delete_message', space_id: id, message_id: entry.id })
                        .catch(() => {});
                  });
                }}
              />
            ) : null}
          </Column>
        </BottomSheet>
      </Host>
    </KeyboardAvoidingView>
  );
}
