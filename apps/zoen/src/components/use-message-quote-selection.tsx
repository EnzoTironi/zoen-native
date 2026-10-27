import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import type { ReplySelection } from '../core/generated/ReplySelection';
import type { TimelineEntry } from '../core/types';
import { useClient } from '../core/provider';
import { messageText } from '../core/presentation';
import { MessageQuoteSelection } from './message-quote-selection';

type SelectionOwner = { entry: TimelineEntry; ownerKey: string };
export function useMessageQuoteSelection(
  current: (entry: TimelineEntry) => TimelineEntry | undefined,
  onReply: (entry: TimelineEntry, selection?: ReplySelection) => void,
  ownerKey: string,
  spaceId: string,
  threadRootId: string | null,
) {
  const { store } = useClient();
  const [selection, setSelection] = useState<SelectionOwner | null>(null);
  const active = useRef<SelectionOwner | null>(null);
  const latest = useRef({ current, onReply, ownerKey });
  useLayoutEffect(() => {
    latest.current = { current, onReply, ownerKey };
    const value = active.current && current(active.current.entry);
    if (
      active.current &&
      (active.current.ownerKey !== ownerKey ||
        !value ||
        value.deleted ||
        value.text_revision !== active.current.entry.text_revision ||
        messageText(value) !== messageText(active.current.entry))
    ) {
      active.current = null;
      setSelection(null);
    }
  }, [current, onReply, ownerKey]);
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
  useLayoutEffect(
    () =>
      store.subscribe(() => {
        if (active.current && !admitted(active.current)) {
          active.current = null;
          setSelection(null);
        }
      }),
    [store, admitted],
  );
  useFocusEffect(
    useCallback(
      () => () => {
        active.current = null;
        setSelection(null);
      },
      [],
    ),
  );
  return {
    open: (entry: TimelineEntry) => {
      const value = latest.current.current(entry);
      if (!value?.text_revision || !('Message' in value.kind) || !messageText(value).trim()) return;
      const captured = {
        ...value,
        author: { ...value.author },
        kind: { Message: { ...value.kind.Message } },
      };
      const selection = { entry: captured, ownerKey: latest.current.ownerKey };
      active.current = selection;
      setSelection(selection);
    },
    panel: selection ? (
      <MessageQuoteSelection
        key={`${selection.ownerKey}:${selection.entry.id}:${selection.entry.text_revision}`}
        entry={selection.entry}
        current={() => admitted(selection)}
        review={async (quote) => {
          if (!admitted(selection)) throw new Error('Selection retired');
          const account = store.getSnapshot().snapshot?.account;
          if (!account) throw new Error('Selection retired');
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
        }}
        onQuote={(quote) => {
          if (admitted(selection)) latest.current.onReply(selection.entry, quote);
        }}
        onClose={() => {
          if (active.current !== selection) return;
          active.current = null;
          setSelection(null);
        }}
      />
    ) : null,
  };
}
