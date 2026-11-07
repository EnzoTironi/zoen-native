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
      let alive = true;
      let foreground = AppState.currentState === 'active';
      let timer: ReturnType<typeof setTimeout> | undefined;
      const observe = () => {
        clearTimeout(timer);
        timer = undefined;
        const observation = session.getSnapshot();
        if (
          !alive ||
          !foreground ||
          !key ||
          observation.scopeKey !== key ||
          observation.phase !== 'ready' ||
          observation.busy ||
          observation.status === 'Revoked'
        )
          return;
        timer = setTimeout(() => {
          if (alive && foreground && AppState.currentState === 'active')
            void session.observe(observation).then((current) => {
              if (current) observe();
            });
        }, 5000);
      };
      // SQL lifecycle controls are not conversation events. Read through the
      // existing authority while visible; these observations never resend an action.
      const observation = session.subscribe(observe);
      const activate = () => {
        if (!alive) return;
        const scope = enabled ? port.scope() : null;
        session.activate(foreground && accountScopeKey(scope) === key ? scope : null);
        if (foreground) void session.refresh();
      };
      activate();
      const owner = store.subscribe(() => {
        if (alive && accountScopeKey(port.scope()) !== key) session.retire();
      });
      const canonical = store.subscribeCanonicalChanges(() => {
        if (alive && foreground) void session.refresh();
      });
      const cleanup = store.registerSessionCleanup(async () => {
        if (alive) session.retire();
      });
      const app = AppState.addEventListener('change', (next) => {
        if (!alive) return;
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (active) activate();
        else session.suspend();
      });
      return () => {
        alive = false;
        observation();
        clearTimeout(timer);
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
