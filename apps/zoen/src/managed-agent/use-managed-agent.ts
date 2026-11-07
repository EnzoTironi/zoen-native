import { useCallback, useMemo, useState, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect, router } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { ManagedAgentSession } from './session';
import { managedAgentPort } from './core-port';

export function useManagedAgent() {
  const { store } = useClient();
  const key = accountScopeKey(accountScope(store));
  const session = useMemo(() => new ManagedAgentSession(managedAgentPort(store)), [store]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const [active, setActive] = useState(false);
  useFocusEffect(
    useCallback(() => {
      let focused = true;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const stop = () => {
        clearTimeout(timer);
        timer = undefined;
        session.retire();
        setActive(false);
      };
      const start = () => {
        if (!focused || AppState.currentState !== 'active') return;
        stop();
        if (process.env.EXPO_OS !== 'ios') return;
        const owner = accountScope(store);
        if (!owner || accountScopeKey(owner) !== key) return;
        session.activate(owner);
        setActive(true);
        void session.refresh();
      };
      // Schedule only after the preceding request completed. Do not continuously
      // retry a failed approval, network error, expired or terminal installation.
      const updates = session.subscribe(() => {
        clearTimeout(timer);
        timer = undefined;
        const value = session.getSnapshot();
        if (
          !focused ||
          !session.scopeMatches(accountScope(store)) ||
          !value.ready ||
          value.busy ||
          value.error
        )
          return;
        const view = value.view;
        const delay =
          view?.state === 'review' && view.preview
            ? Math.min(
                2000,
                Math.max(250, Number(BigInt(view.preview.expires_ms) - BigInt(Date.now())) + 25),
              )
            : view && ['queued', 'installing', 'cancelling'].includes(view.state)
              ? 2000
              : !view && value.recoveries.length === 0
                ? 5000
                : null;
        // Observe another device's consent/completion without resending it. An
        // existing recovery list stays stable while the person chooses a row.
        if (delay !== null) {
          const observation = session.ticket();
          timer = setTimeout(() => {
            if (
              focused &&
              AppState.currentState === 'active' &&
              session.accepts(observation) &&
              session.scopeMatches(accountScope(store))
            )
              void session.refresh();
          }, delay);
        }
      });
      const preflight = store.registerSessionPreflight(() => {
        if (focused) stop();
      });
      const changes = store.subscribe(() => {
        if (focused && accountScopeKey(accountScope(store)) !== key) stop();
      });
      const app = AppState.addEventListener('change', (next) => {
        if (!focused) return;
        if (next === 'active') start();
        else stop();
      });
      start();
      return () => {
        focused = false;
        updates();
        preflight();
        changes();
        app.remove();
        stop();
      };
    }, [session, store, key]),
  );
  const renderedTicket = session.ticket();
  const renderedOperation = state.view?.operation_id;
  const open = async (destination: 'budget' | 'chat') => {
    const ticket = renderedTicket;
    const target = await session.open(destination, ticket, renderedOperation);
    if (!target || !session.accepts(ticket) || !session.scopeMatches(accountScope(store))) return;
    if (destination === 'budget')
      router.push({ pathname: '/account-budget', params: { agent: target } });
    else {
      router.dismissAll();
      router.push({ pathname: '/chat/[id]', params: { id: target } });
    }
  };
  return {
    session,
    state,
    ticket: renderedTicket,
    ownerKey: key,
    available: !!key && process.env.EXPO_OS === 'ios',
    active: active && session.scopeMatches(accountScope(store)),
    open,
  };
}
