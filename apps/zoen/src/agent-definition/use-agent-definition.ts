import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { enrollmentScope } from '../agent-enrollment/session';
import { createDefinitionPort } from './core-port';
import { DefinitionSession, definitionScopeKey, emptyDefinitionState } from './session';

export function useAgentDefinition(agent: string) {
  const { store, mode, snapshot, phase, erasePending } = useClient();
  const accountScope =
    phase === 'ready' && !erasePending && process.env.EXPO_OS === 'ios'
      ? enrollmentScope(mode, snapshot?.account)
      : null;
  const scopeKey = definitionScopeKey(accountScope && agent ? { ...accountScope, agent } : null);
  const port = useMemo(() => createDefinitionPort(store), [store]);
  const session = useMemo(() => new DefinitionSession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useFocusEffect(
    useCallback(() => {
      const current = store.getSnapshot();
      const account =
        current.phase === 'ready' && !current.erasePending && process.env.EXPO_OS === 'ios'
          ? enrollmentScope(current.mode, current.snapshot?.account)
          : null;
      const scope = account && agent ? { ...account, agent } : null;
      session.activate(definitionScopeKey(scope) === scopeKey ? scope : null);
      void session.load();
      const changed = () => {
        const next = store.getSnapshot();
        const account =
          next.phase === 'ready' && !next.erasePending
            ? enrollmentScope(next.mode, next.snapshot?.account)
            : null;
        if (definitionScopeKey(account && agent ? { ...account, agent } : null) !== scopeKey)
          session.retire();
      };
      const unsubscribe = store.subscribe(changed);
      const unregister = store.registerSessionCleanup(async () => {
        session.retire();
      });
      return () => {
        unsubscribe();
        unregister();
        session.retire();
      };
    }, [store, session, agent, scopeKey]),
  );
  return {
    session,
    state:
      state.scope === scopeKey
        ? state
        : { ...emptyDefinitionState, scope: scopeKey, phase: 'unavailable' as const },
  };
}
