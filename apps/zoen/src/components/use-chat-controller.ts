import { type FlashListRef } from '@shopify/flash-list';
import { useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { type ScrollViewInstance, type TextInputInstance } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useDialogs } from '../components/dialogs';
import { useKeyboardMotion } from '../components/keyboard-motion';
import { useMessageActions } from '../components/use-message-actions';
import { ConversationReveal } from '../core/conversation-reveal';
import {
  conversationRevealContent,
  type ConversationContentLayout,
} from '../core/conversation-reveal-content';
import { ConversationSearch } from '../core/conversation-search';
import {
  projectConversationTimeline,
  type ConversationTimelineRow,
} from '../core/conversation-timeline';
import type { UniversalHit } from '../core/generated/UniversalHit';
import { InitialTimelineEnd } from '../core/initial-timeline-end';
import { chatActivityLabel } from '../core/live-activity';
import { messageCard } from '../core/message-presentation';
import { messageText } from '../core/presentation';
import { useClient } from '../core/provider';
import type { TimelineEntry } from '../core/types';
import { useMessageDraft } from '../core/use-message-draft';
import { useHome } from '../home/provider';
import { t } from '../i18n/index.ts';
import { useMessageAudioFocus } from '../media/message-audio-provider';
import { supportsMediaMosaic } from '../media/mosaic-layout';
import { useConversationCards } from '../miniapps/conversation-provider';
import { useTheme } from '../theme';

const revealQaEnabled = process.env.EXPO_PUBLIC_ZOEN_REVEAL_QA === '1';
export function useChatController() {
  const { id, jump } = useLocalSearchParams<{ id: string; jump?: string }>();
  const { store, snapshot, timelines, notice, activity } = useClient();
  const { colors } = useTheme();
  const home = useHome();
  const { allItems: items } = home;
  const cards = useConversationCards(id);
  const headerHeight = useHeaderHeight();
  const { top: statusBarHeight } = useSafeAreaInsets();
  const timelineTopInset = process.env.EXPO_OS === 'ios' ? 0 : statusBarHeight;
  const { progress: keyboardProgress } = useKeyboardMotion();
  const conversationMiniapps = cards.visible;
  const { inform } = useDialogs();
  const input = useRef<TextInputInstance>(null);
  const list = useRef<FlashListRef<ConversationTimelineRow>>(null);
  const focused = useRef(false);
  const normalDraft = useMessageDraft(id);
  const { messageDraft, updateDraft } = normalDraft;
  const { text: draft, reply, editing, editText: editDraft } = messageDraft;
  const [sending, setSending] = useState(false);
  const sendingNow = useRef(false);
  const [loadedSpace, setLoadedSpace] = useState<string | null>(null);
  const [scrollTarget, setScrollTarget] = useState<string | null>(null);
  const [nativeTimeline, setNativeTimeline] = useState<{ scope: string; tag: number } | null>(null);
  const [railHeight, setRailHeight] = useState(0);
  const [noticeHeight, setNoticeHeight] = useState(0);
  const composerText = editing ? editDraft : draft;
  const chat = snapshot?.conversations.find((conversation) => conversation.id === id);
  const owner = [
    store.getSnapshot().mode,
    snapshot?.account?.identity_id ?? snapshot?.me?.id,
    snapshot?.account?.device_id,
  ].join(':');
  useMessageAudioFocus(id, `${owner}:${store.sessionEpoch}`);
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
  const [revealedActivity, setRevealedActivity] = useState<{ scope: string; id: string } | null>(
    null,
  );
  const rows = useMemo(
    () =>
      projectConversationTimeline(
        page?.entries ?? [],
        items,
        id,
        selectedSearchId ??
          (revealedActivity?.scope === `${id}:${owner}` ? revealedActivity.id : null) ??
          jump,
        supportsMediaMosaic,
      ),
    [page, items, id, owner, selectedSearchId, revealedActivity, jump],
  );
  const visibleEntries = useMemo(() => rows.map((row) => row.entry), [rows]);
  const timelineKey = searchState.active && searchThread ? `${id}:${searchThread.root}` : id;
  const nativeTimelineScope = `${timelineKey}:${owner}`;
  const nativeScrollTag = nativeTimeline?.scope === nativeTimelineScope ? nativeTimeline.tag : 0;
  const bindNativeTimeline = useCallback(() => {
    if (process.env.EXPO_OS !== 'ios') return;
    const tag = list.current?.getScrollableNode();
    if (typeof tag !== 'number' || !Number.isSafeInteger(tag) || tag <= 0) return;
    setNativeTimeline((previous) =>
      previous?.scope === nativeTimelineScope && previous.tag === tag
        ? previous
        : { scope: nativeTimelineScope, tag },
    );
  }, [nativeTimelineScope]);
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
    timelineTopInset,
    railHeight,
    noticeHeight,
  });
  const searchReveal = useMemo(() => new ConversationReveal(`${id}:${owner}`), [id, owner]);
  const searchContent = useRef<{
    scope: ConversationReveal;
    id: string;
    layout: ConversationContentLayout;
  } | null>(null);
  const onSearchContentLayout = useCallback(
    (target: string, layout: ConversationContentLayout) => {
      const current = search.getSnapshot();
      if (!current.active || current.hits[current.index]?.ref_id !== target) return;
      searchContent.current = { scope: searchReveal, id: target, layout };
      searchReveal.onItemLayout();
    },
    [search, searchReveal],
  );
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
        const content = searchContent.current;
        return {
          ...conversationRevealContent(
            row,
            content?.scope === searchReveal && content.id === target ? content.layout : null,
          ),
          firstItemOffset: native.getFirstItemOffset(),
          childHeight: native.getChildContainerDimensions().height,
          windowHeight: native.getWindowSize().height,
          bottomAligned: true,
          topObstruction:
            current.headerHeight -
            current.timelineTopInset +
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
      scroll: (offset, animated) => list.current?.scrollToOffset({ offset, animated }),
      trace: revealQaEnabled
        ? (record) => {
            let listOffset: number | null = null;
            try {
              listOffset = list.current?.getAbsoluteLastScrollOffset() ?? null;
            } catch {
              // The public layout getter is unavailable before the first SDK layout.
            }
            console.info(`[ZoenReveal] ${JSON.stringify({ ...record, listOffset })}`);
          }
        : undefined,
    });
  }, [searchReveal, search, keyboardProgress]);
  useLayoutEffect(() => {
    revealLayout.current = {
      entries: visibleEntries,
      ready:
        loadedSpace === timelineKey &&
        !cards.loading &&
        (conversationMiniapps.length === 0 || railHeight > 0),
      headerHeight,
      timelineTopInset,
      railHeight,
      noticeHeight,
    };
    searchReveal.onData(visibleEntries);
  }, [
    page,
    visibleEntries,
    loadedSpace,
    timelineKey,
    cards.loading,
    conversationMiniapps.length,
    headerHeight,
    timelineTopInset,
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
    async (messageId: string, keepActivityVisible = true) => {
      cancelInitialEnd();
      const index = await store.revealMessage(id, messageId);
      if (index < 0) throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
      if (keepActivityVisible) setRevealedActivity({ scope: `${id}:${owner}`, id: messageId });
      return messageId;
    },
    [id, owner, store, cancelInitialEnd],
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
      } else await reveal(selectedHit.ref_id, false);
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
    const index = visibleEntries.findIndex((entry) => entry.id === scrollTarget);
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
            timelineTopInset +
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
    visibleEntries,
    inform,
    headerHeight,
    timelineTopInset,
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
    if (search.getSnapshot().active || sendingNow.current) return;
    const attempt = normalDraft.capture();
    if (!attempt) return;
    const text = attempt.text;
    if (!text.trim() && !(attempt.editingId && editing && messageCard(editing)?.kind_id === 'file'))
      return;
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

  return {
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
    Math,
    noticeHeight,
    items,
    selectedSearchId,
    actions,
    reveal,
    inform,
  };
}
