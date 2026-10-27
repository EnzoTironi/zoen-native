import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState, Platform } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { MemberActionsSession, emptyMemberActions } from './member-actions';
import { createMemberActionsPort, createMemberAdditionPort } from './member-context-port';
import { MemberAdditionSession, emptyMemberAddition } from './member-addition';

export function useMemberActions(space: string, ownedAgentsOnly = false) {
  const client = useClient();
  const { store } = client;
  const port = useMemo(() => createMemberActionsPort(store, space), [store, space]);
  const session = useMemo(() => new MemberActionsSession(space, port), [space, port]);
  const additionPort = useMemo(
    () => createMemberAdditionPort(store, space, ownedAgentsOnly),
    [store, space, ownedAgentsOnly],
  );
  const addition = useMemo(
    () => new MemberAdditionSession(space, additionPort, ownedAgentsOnly),
    [space, additionPort, ownedAgentsOnly],
  );
  const additionState = useSyncExternalStore(
    addition.subscribe,
    addition.getSnapshot,
    addition.getSnapshot,
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const scope = Platform.OS === 'web' ? '' : accountScopeKey(accountScope(store, client));
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const owner = foreground && accountScopeKey(port.scope()) === scope ? port.scope() : null;
        session.activate(owner);
        addition.activate(owner);
      };
      const retire = () => {
        session.retire();
        addition.retire();
      };
      activate();
      const account = store.subscribe(() => {
        if (accountScopeKey(port.scope()) !== scope) retire();
      });
      const canonical = store.subscribeCanonicalChanges((spaces) => {
        if (foreground && (!spaces || spaces.includes(space))) {
          void session.refresh(session.ticket());
          void addition.refresh();
        }
      });
      const preflight = store.registerSessionPreflight(retire);
      const cleanup = store.registerSessionCleanup(async () => retire());
      const lifecycle = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (foreground) activate();
        else retire();
      });
      return () => {
        account();
        canonical();
        preflight();
        cleanup();
        lifecycle.remove();
        retire();
      };
    }, [store, space, scope, port, session, addition]),
  );
  return {
    session,
    addition,
    additionState: additionState.scope === scope ? additionState : emptyMemberAddition,
    available: Boolean(scope),
    state: state.scope === scope ? state : emptyMemberActions,
  };
}
