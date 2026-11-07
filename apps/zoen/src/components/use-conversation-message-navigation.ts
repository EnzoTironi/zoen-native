import { useCallback, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { useFocusEffect } from 'expo-router';
import { conversationScope } from '../core/conversation-scope';
import type { ClientStore } from '../core/store';
import type { ConversationReveal } from '../core/conversation-reveal';
import { t } from '../i18n/index.ts';

/** Transient navigation only. Core owns history; the shared scroller owns geometry. */
export function useConversationMessageNavigation({
  store,
  space,
  owner,
  focused,
  scroller,
  cancelInitialEnd,
}: {
  store: ClientStore;
  space: string;
  owner: string | null;
  focused: RefObject<boolean>;
  scroller: ConversationReveal;
  cancelInitialEnd(): void;
}) {
  const [selection, setSelection] = useState<{
    owner: string;
    scroller: ConversationReveal;
    id: string;
  } | null>(null);
  const active = useRef<ConversationReveal | null>(null);
  useLayoutEffect(() => {
    active.current = scroller;
    return () => {
      if (active.current === scroller) active.current = null;
    };
  }, [scroller]);
  const current = useCallback(
    () =>
      active.current === scroller &&
      focused.current &&
      owner !== null &&
      !store.isSessionRetiring &&
      conversationScope(store.getSnapshot(), store.sessionEpoch, space) === owner,
    [focused, owner, store, space, scroller],
  );
  useFocusEffect(
    useCallback(() => {
      const unsubscribe = store.subscribe(() => {
        if (!current()) {
          scroller.cancel();
          setSelection(null);
        }
      });
      return () => {
        unsubscribe();
        scroller.cancel();
        setSelection(null);
      };
    }, [store, current, scroller]),
  );
  const select = useCallback(
    (id: string | null) => {
      if (current()) setSelection(id ? { owner: owner!, scroller, id } : null);
    },
    [current, owner, scroller],
  );
  const reveal = useCallback(
    async (id: string, explicit = true, admitted?: () => boolean) => {
      if (!current()) return null;
      cancelInitialEnd();
      if (explicit) scroller.cancel();
      const token = scroller.capture();
      const valid = () => current() && scroller.isCurrent(token) && (admitted?.() ?? true);
      const result = await store.revealMessage(space, id, valid).then(
        (index) => ({ ok: true, index }) as const,
        (reason: unknown) => ({ ok: false, reason }) as const,
      );
      if (!valid()) return null;
      if (!result.ok) throw result.reason;
      if (result.index < 0) throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
      return id;
    },
    [current, cancelInitialEnd, scroller, store, space],
  );
  return {
    target: selection?.owner === owner && selection.scroller === scroller ? selection.id : null,
    select,
    reveal,
    current,
  };
}
