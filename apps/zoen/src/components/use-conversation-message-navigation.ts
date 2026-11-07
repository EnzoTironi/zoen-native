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
  locate,
}: {
  store: ClientStore;
  space: string;
  owner: string | null;
  focused: RefObject<boolean>;
  scroller: ConversationReveal;
  cancelInitialEnd(): void;
  locate?(id: string, current: () => boolean): Promise<boolean>;
}) {
  const [selection, setSelection] = useState<{
    owner: string;
    scroller: ConversationReveal;
    id: string | null;
    trail: readonly string[];
  } | null>(null);
  const active = useRef<ConversationReveal | null>(null);
  const latest = useRef(selection);
  const returning = useRef<object | null>(null);
  const [pending, setPending] = useState<{ token: object; scroller: ConversationReveal } | null>(
    null,
  );
  useLayoutEffect(() => {
    latest.current = selection;
  }, [selection]);
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
          returning.current = null;
          setPending(null);
        }
      });
      return () => {
        unsubscribe();
        scroller.cancel();
        setSelection(null);
        returning.current = null;
        setPending(null);
      };
    }, [store, current, scroller]),
  );
  const select = useCallback(
    (id: string | null, origin?: string) => {
      if (!current()) return;
      setSelection((previous) => {
        const old = previous?.owner === owner && previous.scroller === scroller ? previous : null;
        const trail =
          origin && origin !== id ? [...(old?.trail ?? []), origin] : (old?.trail ?? []);
        return id || old ? { owner: owner!, scroller, id, trail } : null;
      });
    },
    [current, owner, scroller],
  );
  const restore = useCallback(async () => {
    if (!current() || !selection || latest.current !== selection || returning.current) return null;
    const origin = selection.trail.at(-1);
    if (!origin) return null;
    const token = {};
    returning.current = token;
    setPending({ token, scroller });
    cancelInitialEnd();
    scroller.cancel();
    const generation = scroller.capture();
    const valid = () =>
      current() &&
      latest.current === selection &&
      returning.current === token &&
      scroller.isCurrent(generation);
    const result = await (
      locate
        ? locate(origin, valid)
        : store.revealMessage(space, origin, valid).then((index) => index >= 0)
    ).then(
      (found) => ({ ok: true, found }) as const,
      (reason: unknown) => ({ ok: false, reason }) as const,
    );
    const accepted = valid();
    if (returning.current === token) {
      returning.current = null;
      setPending(null);
    }
    if (!accepted) return null;
    if (!result.ok) throw result.reason;
    setSelection((value) =>
      value === selection
        ? { ...value, id: result.found ? origin : null, trail: value.trail.slice(0, -1) }
        : value,
    );
    if (!result.found) throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
    return origin;
  }, [current, selection, cancelInitialEnd, scroller, locate, store, space]);
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
    restore,
    canReturn:
      selection?.owner === owner && selection.scroller === scroller && selection.trail.length > 0,
    returning: pending?.scroller === scroller,
    reveal,
    current,
  };
}
