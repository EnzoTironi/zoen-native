import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { router, useFocusEffect } from 'expo-router';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { useClient } from '../core/provider';
import { SavedMessagesSession } from './session';

export function useSavedMessages() {
  const client = useClient();
  const { store } = client;
  const session = useMemo(
    () =>
      new SavedMessagesSession({
        scope: () => accountScope(store),
        summary: () =>
          store.getSnapshot().snapshot?.conversations.find((chat) => chat.kind === 'saved'),
        ensure: (scope) =>
          store.request({
            op: 'saved_messages',
            expected_identity_id: scope.identity,
            expected_device_id: scope.device,
          }),
        open: (id) => {
          router.replace({ pathname: '/chat/[id]', params: { id } });
        },
      }),
    [store],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const scopeKey = accountScopeKey(accountScope(store, client));
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const scope = accountScope(store);
        session.activate(foreground && accountScopeKey(scope) === scopeKey ? scope : null);
      };
      const unsubscribe = store.subscribe(() => {
        if (accountScopeKey(accountScope(store)) !== scopeKey) session.retire();
        else if (foreground) session.observe();
      });
      const unregister = store.registerSessionPreflight(() => session.retire());
      const appState = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (foreground) activate();
        else session.retire();
      });
      activate();
      return () => {
        appState.remove();
        unsubscribe();
        unregister();
        session.retire();
      };
    }, [store, session, scopeKey]),
  );
  return {
    retry: () => session.retry(state.revision),
    phase: state.scope === scopeKey ? state.phase : 'unavailable',
  };
}
