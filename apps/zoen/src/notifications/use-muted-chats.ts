import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { MutedChatsSession, createMutedChatsPort, emptyMutedChats } from './muted-chats';

export function useMutedChats() {
  const client = useClient();
  const { store } = client;
  const port = useMemo(() => createMutedChatsPort(store), [store]);
  const session = useMemo(() => new MutedChatsSession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const scopeKey = accountScopeKey(accountScope(store, client));
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      let focused = true;
      const activate = () => {
        const owner = port.scope();
        session.activate(
          foreground && focused && accountScopeKey(owner) === scopeKey ? owner : null,
        );
        if (foreground && focused) void session.load(session.ticket());
      };
      activate();
      const unsubscribe = store.subscribe(() => {
        if (accountScopeKey(port.scope()) !== scopeKey) session.retire();
      });
      const canonical = store.subscribeCanonicalChanges(() => {
        if (foreground && focused) void session.load(session.ticket());
      });
      const unregister = store.registerSessionPreflight(() => session.retire());
      const cleanup = store.registerSessionCleanup(async () => session.retire());
      const subscription = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        focused = false;
        subscription.remove();
        unsubscribe();
        canonical();
        unregister();
        cleanup();
        session.retire();
      };
    }, [store, port, session, scopeKey]),
  );
  return {
    session,
    accountRequired: !scopeKey,
    state: state.scope === scopeKey ? state : { ...emptyMutedChats, scope: scopeKey },
  };
}
