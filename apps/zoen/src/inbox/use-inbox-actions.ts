import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { useClient } from '../core/provider';
import type { ClientStore } from '../core/store';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { InboxActionsSession, type InboxActionsPort } from './actions-session';

export function inboxActionsPort(store: ClientStore): InboxActionsPort {
  return {
    scope: () => accountScope(store),
    member: (id) => {
      const owner = accountScope(store);
      return Boolean(
        owner &&
        store
          .getSnapshot()
          .snapshot?.conversations.some(
            (chat) => chat.id === id && chat.members.some((m) => m.id === owner.identity),
          ),
      );
    },
    snapshot: () => store.driver.request({ op: 'snapshot' }),
    layout: (input) =>
      input.kind === 'read' || input.kind === 'reconcile'
        ? store.driver.request({ op: 'chat_layout', input })
        : store.request({ op: 'chat_layout', input }),
    newId: () => store.driver.request({ op: 'new_mutation_id' }),
    mute: (space_id, muted, expected_muted_until_ms, owner) => {
      if (accountScopeKey(owner) !== accountScopeKey(accountScope(store)))
        throw Error('Account changed');
      return store.request({
        op: 'set_chat_mute',
        space_id,
        mode: { kind: muted ? 'forever' : 'off' },
        expected_muted_until_ms,
        expected_identity_id: owner.identity,
        expected_device_id: owner.device,
      });
    },
    refresh: () => store.refresh(),
  };
}
export function useInboxActions() {
  const client = useClient(),
    { store } = client;
  const port = useMemo(() => inboxActionsPort(store), [store]);
  const session = useMemo(() => new InboxActionsSession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const scope = process.env.EXPO_OS === 'web' ? '' : accountScopeKey(accountScope(store, client));
  const navigationOwner = JSON.stringify([
    store.sessionEpoch,
    homeOwnerFor(client),
    client.snapshot?.account?.relay_url ?? null,
  ]);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const owner = port.scope();
        session.activate(foreground && scope && accountScopeKey(owner) === scope ? owner : null);
      };
      activate();
      const stop = store.subscribe(() => {
        const current = session.getSnapshot();
        if (
          accountScopeKey(port.scope()) !== scope ||
          (current.pending && !port.member(current.pending)) ||
          (current.undo && !port.member(current.undo.id))
        )
          session.retire();
      });
      const preflight = store.registerSessionPreflight(() => session.retire());
      const app = AppState.addEventListener('change', (next) => {
        if ((next === 'active') === foreground) return;
        foreground = next === 'active';
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        stop();
        preflight();
        app.remove();
        session.retire();
      };
    }, [store, port, session, scope]),
  );
  return {
    session,
    state: state.scope === scope ? state : { scope, pending: null, undo: null, error: null },
    ticket: session.ticket(state),
    enabled: Boolean(scope && state.scope === scope),
    canOpen: (id: string) => {
      if (scope) return session.canOpen(id, session.ticket(state));
      const current = store.getSnapshot();
      return (
        (current.mode === 'demo' || process.env.EXPO_OS === 'web') &&
        !store.isSessionRetiring &&
        AppState.currentState === 'active' &&
        navigationOwner ===
          JSON.stringify([
            store.sessionEpoch,
            homeOwnerFor(current),
            current.snapshot?.account?.relay_url ?? null,
          ]) &&
        homeSpacesFor(current).has(id)
      );
    },
  };
}
