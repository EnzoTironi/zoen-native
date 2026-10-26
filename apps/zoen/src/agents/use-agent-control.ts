import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { useClient } from '../core/provider';
import { AgentControlSession, emptyAgentControl } from './agent-control-session';
import { createAgentControlPort } from './agent-control-port';

const sessions = new WeakMap<object, Map<string, AgentControlSession>>();

function sessionFor(store: object, agent: string, port: ReturnType<typeof createAgentControlPort>) {
  let agents = sessions.get(store);
  if (!agents) {
    agents = new Map();
    sessions.set(store, agents);
  }
  let session = agents.get(agent);
  if (!session) {
    session = new AgentControlSession(port, agent);
    agents.set(agent, session);
  }
  return session;
}

export function useAgentControl(agent: string, enabled: boolean) {
  const client = useClient();
  const { store } = client;
  const key = accountScopeKey(enabled ? accountScope(store, client) : null);
  const port = useMemo(() => createAgentControlPort(store), [store]);
  const session = useMemo(() => sessionFor(store, agent, port), [store, port, agent]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const scope = enabled ? port.scope() : null;
        session.activate(foreground && accountScopeKey(scope) === key ? scope : null);
        if (foreground) void session.refresh();
      };
      activate();
      const owner = store.subscribe(() => {
        if (accountScopeKey(port.scope()) !== key) session.retire();
      });
      const canonical = store.subscribeCanonicalChanges(() => {
        if (foreground) void session.refresh();
      });
      const cleanup = store.registerSessionCleanup(async () => {
        session.retire();
      });
      const app = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (active) activate();
        else session.suspend();
      });
      return () => {
        owner();
        canonical();
        cleanup();
        app.remove();
        session.suspend();
      };
    }, [store, session, port, key, enabled]),
  );
  return { session, state: key && state.scopeKey === key ? state : emptyAgentControl };
}
