import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from 'react';
import { router, useFocusEffect, useIsFocused } from 'expo-router';
import type { FlashListRef } from '@shopify/flash-list';
import { useClient } from '../core/provider';
import { conversationScope } from '../core/conversation-scope';
import { ConversationReveal } from '../core/conversation-reveal';
import {
  conversationRevealContent,
  type ConversationContentLayout,
} from '../core/conversation-reveal-content';
import {
  projectConversationTimeline,
  type ConversationTimelineRow,
} from '../core/conversation-timeline';
import { resolveMessageLink } from '../core/message-link';
import type { TimelineEntry, ItemDetail } from '../core/types';
import { supportsMediaMosaic } from '../media/mosaic-layout';
import { useKeyboardMotion } from './keyboard-motion';
import { useChatNativeMetrics } from './use-chat-native-metrics';
import { useConversationMessageNavigation } from './use-conversation-message-navigation';
import { t } from '../i18n/index.ts';

const noAction = () => {};
/** A thread shares the chat's geometry and canonical navigation authority. */
export function useThreadMessageNavigation({
  space,
  root,
  jump,
  entries,
  items,
  list,
  disabled,
  headerHeight,
  timelineTopInset,
  reply,
  editing,
}: {
  space: string;
  root: string;
  jump?: string;
  entries?: readonly TimelineEntry[] | null;
  items: readonly ItemDetail[];
  list: RefObject<FlashListRef<ConversationTimelineRow> | null>;
  disabled: boolean;
  headerHeight: number;
  timelineTopInset: number;
  reply: boolean;
  editing: boolean;
}) {
  const client = useClient(),
    { store } = client;
  const owner = conversationScope(client, store.sessionEpoch, space);
  const scroller = useMemo(
    () => new ConversationReveal(JSON.stringify([owner, root])),
    [owner, root],
  );
  const focused = useRef(false);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
      };
    }, []),
  );
  const sources = useRef(entries);
  useLayoutEffect(() => {
    sources.current = entries;
  }, [entries]);
  const locate = useCallback(
    async (id: string, admitted: () => boolean) =>
      admitted() && sources.current?.some((entry) => entry.id === id) === true,
    [],
  );
  const navigation = useConversationMessageNavigation({
    store,
    space,
    owner,
    focused,
    scroller,
    cancelInitialEnd: noAction,
    locate,
  });
  const { target, current, select } = navigation;
  const rows = useMemo<ConversationTimelineRow[]>(
    () =>
      disabled
        ? (entries ?? []).map((entry) => ({ key: entry.id, entry }))
        : projectConversationTimeline(
            entries ?? [],
            items,
            space,
            target ?? root,
            supportsMediaMosaic,
          ),
    [disabled, entries, items, space, target, root],
  );
  const visible = useMemo(() => rows.map((row) => row.entry), [rows]);
  const [loaded, setLoaded] = useState<ConversationReveal | null>(null);
  const [native, setNative] = useState<{ scroller: ConversationReveal; tag: number } | null>(null);
  const [failure, setFailure] = useState<{ scroller: ConversationReveal; message: string } | null>(
    null,
  );
  const consumed = useRef<{ scroller: ConversationReveal; id: string } | null>(null);
  useEffect(() => {
    if (
      disabled ||
      !jump ||
      !current() ||
      (consumed.current?.scroller === scroller && consumed.current.id === jump) ||
      !entries?.some((entry) => entry.id === jump)
    )
      return;
    consumed.current = { scroller, id: jump };
    select(jump);
  }, [disabled, jump, current, scroller, entries, select]);
  useEffect(() => {
    if (disabled) {
      scroller.cancel();
      select(null);
    }
  }, [disabled, scroller, select]);
  const metrics = useChatNativeMetrics({
    reveal: scroller,
    focused: useIsFocused(),
    searchActive: false,
    revealActive: Boolean(target) && !disabled,
    current,
    list,
    rows: rows.length,
    keyboard: useKeyboardMotion(),
    reply,
    editing,
  });
  const latest = useRef({ visible, target, ready: false, headerHeight, timelineTopInset });
  const content = useRef<{
    scroller: ConversationReveal;
    id: string;
    layout: ConversationContentLayout;
  } | null>(null);
  useLayoutEffect(() => {
    latest.current = {
      visible,
      target,
      ready: loaded === scroller && !disabled,
      headerHeight,
      timelineTopInset,
    };
    scroller.onData(visible);
  }, [visible, target, loaded, scroller, disabled, headerHeight, timelineTopInset]);
  useLayoutEffect(() => {
    scroller.connect({
      current: (id) => current() && !disabled && latest.current.target === id,
      observe: metrics.observe,
      item: (id) => {
        const values = latest.current,
          sdk = list.current;
        const index = values.visible.findIndex((entry) => entry.id === id);
        if (!values.ready || !sdk || index < 0) return null;
        const row = sdk.getLayout(index);
        if (!row) return null;
        const measured = content.current;
        return {
          ...conversationRevealContent(
            row,
            measured?.scroller === scroller && measured.id === id ? measured.layout : null,
          ),
          firstItemOffset: sdk.getFirstItemOffset(),
          childHeight: sdk.getChildContainerDimensions().height,
          windowHeight: sdk.getWindowSize().height,
          bottomAligned: false,
          topObstruction: Math.max(0, values.headerHeight - values.timelineTopInset) + 8,
        };
      },
      frame: (work) => {
        const id = requestAnimationFrame(work);
        return () => cancelAnimationFrame(id);
      },
      scroll: (offset, animated) => list.current?.scrollToOffset({ offset, animated }),
    });
    return () => scroller.cancel();
  }, [scroller, current, disabled, list, metrics.observe]);
  useEffect(() => {
    if (
      process.env.EXPO_OS === 'ios' &&
      target &&
      loaded === scroller &&
      !disabled &&
      !navigation.returning
    )
      scroller.request(target);
  }, [target, loaded, scroller, disabled, visible, navigation.returning]);
  const bind = useCallback(() => {
    if (!current()) return;
    const tag = list.current?.getScrollableNode();
    if (typeof tag === 'number' && Number.isSafeInteger(tag) && tag > 0)
      setNative((value) =>
        value?.scroller === scroller && value.tag === tag ? value : { scroller, tag },
      );
  }, [list, scroller, current]);
  const cancel = useCallback(() => {
    if (!current()) return;
    scroller.cancel();
    select(null);
    setFailure(null);
  }, [scroller, select, current]);
  const quote = useCallback(
    (id: string, sourceSpace?: string, originId?: string) => {
      if (disabled || !current()) return;
      cancel();
      if ((!sourceSpace || sourceSpace === space) && entries?.some((entry) => entry.id === id)) {
        select(id, originId);
        if (process.env.EXPO_OS !== 'ios') {
          const index = rows.findIndex(
            (row) => row.entry.id === id || row.album?.entries.some((entry) => entry.id === id),
          );
          if (index >= 0)
            void list.current
              ?.scrollToIndex({ index, animated: true, viewPosition: 0.5 })
              .catch(() => {});
        }
        return;
      }
      const token = scroller.capture();
      const admitted = () => current() && scroller.isCurrent(token);
      void resolveMessageLink(store, sourceSpace ?? space, id, admitted)
        .then((destination) => {
          if (!admitted()) return;
          if (destination) router.push(destination);
          else setFailure({ scroller, message: t('chat.messageUnavailable') });
        })
        .catch(() => {
          if (admitted()) setFailure({ scroller, message: t('chat.messageUnavailable') });
        });
    },
    [disabled, current, cancel, space, entries, select, rows, list, scroller, store],
  );
  return {
    rows,
    target,
    returnNavigation: navigation,
    error: failure?.scroller === scroller ? failure.message : null,
    metrics: metrics.navigationMetrics,
    scrollViewTag: native?.scroller === scroller ? native.tag : 0,
    onComposerLayout: metrics.onComposerLayout,
    onLoad: () => {
      if (!current()) return;
      bind();
      setLoaded(scroller);
    },
    onLayout: (height: number) => {
      if (!current()) return;
      bind();
      scroller.onLayout(height);
    },
    onInset: (inset: { top: number; bottom: number }) => {
      if (!current()) return;
      scroller.onInset(inset);
      metrics.onInsetRequested(inset);
    },
    onScroll: scroller.onScroll,
    onContentSize: scroller.onContentSize,
    onCommit: () => scroller.onCommit(visible),
    onContentLayout: (id: string, layout: ConversationContentLayout) => {
      if (!current() || latest.current.target !== id) return;
      content.current = { scroller, id, layout };
      scroller.onItemLayout();
    },
    cancel,
    quote,
  };
}
