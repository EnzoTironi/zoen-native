import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import {
  BlockedPeopleSession,
  blockedPeopleScopeKey,
  createBlockedPeoplePort,
  emptyBlockedPeople,
} from './blocked-people';

export function useBlockedPeople() {
  const { store } = useClient();
  const port = useMemo(() => createBlockedPeoplePort(store), [store]);
  const session = useMemo(() => new BlockedPeopleSession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const scopeKey = blockedPeopleScopeKey(port.scope());
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const scope = port.scope();
        session.activate(foreground && blockedPeopleScopeKey(scope) === scopeKey ? scope : null);
        if (foreground) void session.load(session.ticket());
      };
      activate();
      const unsubscribe = store.subscribe(() => {
        if (blockedPeopleScopeKey(port.scope()) !== scopeKey) session.retire();
      });
      const unregister = store.registerSessionPreflight(() => session.retire());
      const appState = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        appState.remove();
        unsubscribe();
        unregister();
        session.retire();
      };
    }, [store, port, session, scopeKey]),
  );
  return {
    session,
    accountRequired: !scopeKey,
    state: state.scope === scopeKey ? state : { ...emptyBlockedPeople, scope: scopeKey },
  };
}
