import { useCallback, useLayoutEffect, useRef, type ReactNode } from 'react';
import { useFocusEffect } from 'expo-router';
import type { ConversationSearch } from '../core/conversation-search';
import { messageHashtag } from '../core/message-hashtag';
import { ConversationHashtagSearch } from './conversation-hashtag-search';

/** One stable action per chat owner; unrelated renders do not retire open links. */
export function ConversationHashtagProvider({
  search,
  onActivate,
  children,
}: {
  search: ConversationSearch;
  onActivate(): void;
  children: ReactNode;
}) {
  const focused = useRef(false);
  const activation = useRef<{ search: ConversationSearch; run(): void } | null>(null);
  useLayoutEffect(() => {
    activation.current = { search, run: onActivate };
    return () => {
      activation.current = null;
    };
  }, [onActivate, search]);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
      };
    }, []),
  );
  const open = useCallback(
    (tag: string) => {
      if (
        !focused.current ||
        !activation.current ||
        activation.current.search !== search ||
        !messageHashtag(tag) ||
        !search.openQuery(tag)
      )
        return;
      activation.current.run();
    },
    [search],
  );
  return <ConversationHashtagSearch value={open}>{children}</ConversationHashtagSearch>;
}
