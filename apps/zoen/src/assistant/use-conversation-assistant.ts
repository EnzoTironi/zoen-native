import { useCallback, useRef, useState } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { conversationScope } from '../core/conversation-scope';
import { useClient } from '../core/provider';
import { EphemeralConversation } from './conversation';

/** A temporary model lease belongs to one conversation owner and focused foreground activation. */
export function useConversationAssistant(
  space: string,
  question: string,
  locale: 'en' | 'pt-BR',
  enabled = true,
) {
  const client = useClient();
  const { store } = client;
  const port = store.driver.assistant;
  const scope = conversationScope(client, store.sessionEpoch, space);
  const [origin] = useState(scope);
  const [retired, setRetired] = useState(false);
  const seeded = useRef(false);
  const drain = useRef<Promise<void>>(Promise.resolve());
  const retryClose = useRef<(() => Promise<void>) | null>(null);
  const [cleanupFailed, setCleanupFailed] = useState(false);
  const [owner, setOwner] = useState<EphemeralConversation | null>(null);
  const [draft, setDraft] = useState('');
  const [inputError, setInputError] = useState(false);
  const [restart, setRestart] = useState(0);
  const current = useCallback(
    () =>
      enabled &&
      !retired &&
      !!origin &&
      !store.isSessionRetiring &&
      conversationScope(store.getSnapshot(), store.sessionEpoch, space) === origin,
    [store, space, enabled, origin, retired],
  );
  useFocusEffect(
    useCallback(() => {
      if (scope !== origin || retired) return;
      let active = true;
      let foreground = AppState.currentState === 'active';
      let generation = 0;
      let owned: EphemeralConversation | null = null;
      let closeOwned = () => Promise.resolve();
      const close = () => {
        ++generation;
        const closing = owned;
        owned = null;
        setOwner(null);
        if (current()) {
          const pending = closing?.getSnapshot().pending;
          if (pending) setDraft((value) => value || pending);
        } else {
          setRetired(true);
          setDraft('');
        }
        if (closing) {
          const closeLease = closeOwned;
          retryClose.current = closeLease;
          drain.current = closeLease();
          void drain.current
            .then(() => {
              if (retryClose.current === closeLease) retryClose.current = null;
            })
            .catch(() => {});
        }
        return drain.current;
      };
      const open = async () => {
        const opening = ++generation;
        try {
          await drain.current;
        } catch {
          // An unacknowledged close owns the old lease; retry only its cleanup explicitly.
          if (active && current()) setCleanupFailed(true);
          return;
        }
        if (!active || !foreground || opening !== generation || !port || !current()) return;
        const conversation = new EphemeralConversation(port);
        owned = conversation;
        let unregister = () => {};
        closeOwned = async () => {
          await conversation.close();
          unregister();
        };
        unregister = store.registerSessionCleanup(closeOwned);
        const initial = restart === 0 && !seeded.current ? question : '';
        seeded.current = true;
        if (initial) setDraft(initial);
        await conversation.open(space, locale);
        if (!active || !foreground || opening !== generation || !current()) return;
        setOwner(conversation);
        if (!initial) return;
        if (conversation.getSnapshot().phase !== 'ready') return;
        if (new TextEncoder().encode(initial).byteLength > 512) {
          setInputError(true);
          return;
        }
        setDraft('');
        const accepted = await conversation.ask(initial);
        if (active && foreground && opening === generation && current() && !accepted)
          setDraft((value) => value || initial);
      };
      const unsubscribe = store.subscribe(() => {
        if (!current()) void close().catch(() => {});
      });
      const app = AppState.addEventListener('change', (state) => {
        const next = state === 'active';
        if (foreground === next) return;
        foreground = next;
        if (next) void open();
        else void close().catch(() => {});
      });
      void open();
      return () => {
        active = false;
        unsubscribe();
        app.remove();
        void close().catch(() => {});
      };
    }, [store, port, space, scope, origin, retired, question, locale, restart, current]),
  );
  return {
    owner: scope === origin && !retired ? owner : null,
    available: enabled && !!port && scope === origin && !retired && !!scope,
    reason: port ? ('not_ready' as const) : ('unsupported' as const),
    cleanupFailed,
    draft,
    setDraft,
    inputError,
    setInputError,
    restart: () => {
      if (retryClose.current) drain.current = retryClose.current();
      setCleanupFailed(false);
      setInputError(false);
      setRestart((value) => value + 1);
    },
  };
}
