import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState, Platform } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScopeKey } from '../core/account-scope';
import { MemberActionsSession, emptyMemberActions } from './member-actions';
import { createMemberActionsPort } from './member-context-port';

export function useMemberActions(space: string) {
  const { store } = useClient();
  const port = useMemo(() => createMemberActionsPort(store, space), [store, space]);
  const session = useMemo(() => new MemberActionsSession(space, port), [space, port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const scope = Platform.OS === 'web' ? '' : accountScopeKey(port.scope());
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () =>
        session.activate(
          foreground && accountScopeKey(port.scope()) === scope ? port.scope() : null,
        );
      activate();
      const account = store.subscribe(() => {
        if (accountScopeKey(port.scope()) !== scope) session.retire();
      });
      const canonical = store.subscribeCanonicalChanges((spaces) => {
        if (foreground && (!spaces || spaces.includes(space)))
          void session.refresh(session.ticket());
      });
      const preflight = store.registerSessionPreflight(() => session.retire());
      const cleanup = store.registerSessionCleanup(async () => session.retire());
      const lifecycle = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        account();
        canonical();
        preflight();
        cleanup();
        lifecycle.remove();
        session.retire();
      };
    }, [store, space, scope, port, session]),
  );
  return {
    session,
    available: Boolean(scope),
    state: state.scope === scope ? state : emptyMemberActions,
  };
}
