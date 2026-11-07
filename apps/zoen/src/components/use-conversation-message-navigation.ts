import { useCallback, useState, type RefObject } from 'react';
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
  const [selection, setSelection] = useState<{ owner: string; id: string } | null>(null);
  const current = useCallback(
    () =>
      focused.current &&
      owner !== null &&
      !store.isSessionRetiring &&
      conversationScope(store.getSnapshot(), store.sessionEpoch, space) === owner,
    [focused, owner, store, space],
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
      if (current()) setSelection(id ? { owner: owner!, id } : null);
    },
    [current, owner],
  );
  const reveal = useCallback(
    async (id: string, explicit = true, admitted?: () => boolean) => {
      if (!current()) return null;
      cancelInitialEnd();
      if (explicit) scroller.cancel();
      const token = scroller.capture();
      const valid = () => current() && scroller.isCurrent(token) && (admitted?.() ?? true);
      try {
        const index = await store.revealMessage(space, id, valid);
        if (!valid()) return null;
        if (index < 0) throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
        return id;
      } catch (reason) {
        if (!valid()) return null;
        throw reason;
      }
    },
    [current, cancelInitialEnd, scroller, store, space],
  );
  return { target: selection?.owner === owner ? selection.id : null, select, reveal, current };
}
