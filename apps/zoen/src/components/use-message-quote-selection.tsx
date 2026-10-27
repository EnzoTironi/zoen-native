import { useCallback, useLayoutEffect, useRef } from 'react';
import { useFocusEffect } from 'expo-router';
import type { ReplySelection } from '../core/generated/ReplySelection';
import type { TimelineEntry } from '../core/types';
import { useClient } from '../core/provider';
import { messageText } from '../core/presentation';
import { selectedMessageQuote } from '../core/selected-message-quote';

type SelectionOwner = { entry: TimelineEntry; ownerKey: string };
export function useMessageQuoteSelection(
  current: (entry: TimelineEntry) => TimelineEntry | undefined,
  onReply: (entry: TimelineEntry, selection?: ReplySelection) => void,
  ownerKey: string,
  spaceId: string,
  threadRootId: string | null,
) {
  const { store } = useClient();
  const active = useRef<SelectionOwner | null>(null);
  const latest = useRef({ current, onReply, ownerKey });
  const admitted = useCallback((selection: SelectionOwner) => {
    if (active.current !== selection || latest.current.ownerKey !== selection.ownerKey)
      return false;
    const value = latest.current.current(selection.entry);
    return (
      value?.text_revision === selection.entry.text_revision &&
      !value?.deleted &&
      messageText(value!) === messageText(selection.entry)
    );
  }, []);
  useLayoutEffect(() => {
    latest.current = { current, onReply, ownerKey };
    if (active.current && !admitted(active.current)) active.current = null;
  }, [current, onReply, ownerKey, admitted]);
  useLayoutEffect(
    () =>
      store.subscribe(() => {
        if (active.current && !admitted(active.current)) active.current = null;
      }),
    [store, admitted],
  );
  useFocusEffect(
    useCallback(
      () => () => {
        active.current = null;
      },
      [],
    ),
  );
  return {
    open: (entry: TimelineEntry) => {
      if (latest.current.ownerKey !== ownerKey) return;
      const value = latest.current.current(entry);
      if (!value?.text_revision || !('Message' in value.kind) || !messageText(value).trim()) return;
      const selection = {
        entry: {
          ...value,
          author: { ...value.author },
          kind: { Message: { ...value.kind.Message } },
        },
        ownerKey: latest.current.ownerKey,
      };
      active.current = selection;
      return () => {
        if (active.current === selection) active.current = null;
      };
    },
    cancel: () => {
      active.current = null;
    },
    review: async (entry: TimelineEntry, text: string, ranges: string) => {
      const selection = active.current;
      if (
        !selection ||
        selection.entry.id !== entry.id ||
        selection.ownerKey !== ownerKey ||
        !admitted(selection)
      )
        throw new Error('Selection retired');
      const quote = selectedMessageQuote(selection.entry, text, ranges);
      const account = store.getSnapshot().snapshot?.account;
      if (!quote || !account) throw new Error('Selection unavailable');
      const result = await store.driver.request({
        op: 'review_reply_selection',
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: spaceId,
        thread_root_id: threadRootId,
        selection: quote,
      });
      if (!admitted(selection)) throw new Error('Selection retired');
      return result;
    },
    accept: (entry: TimelineEntry, quote: ReplySelection) => {
      const selection = active.current;
      if (
        !selection ||
        selection.entry.id !== entry.id ||
        selection.ownerKey !== ownerKey ||
        !admitted(selection)
      )
        return;
      const checked = selectedMessageQuote(
        selection.entry,
        quote.text,
        JSON.stringify(quote.spans),
      );
      if (!checked || checked.revision !== quote.revision || checked.source_id !== quote.source_id)
        return;
      latest.current.onReply(selection.entry, checked);
      active.current = null;
    },
  };
}
