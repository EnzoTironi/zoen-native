import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { createOwnerPolicyPort, policyAvailability } from './core-port';
import { OwnerPolicySession, emptyPolicyState, policyScopeKey, type PolicyTarget } from './session';

export function useOwnerPolicy({
  spaceId,
  agentId,
  requestId,
  expectedHash,
  purpose,
}: PolicyTarget) {
  const { store } = useClient();
  const target = useMemo(
    (): PolicyTarget =>
      purpose === 'account'
        ? { spaceId, agentId, purpose }
        : { spaceId, agentId, requestId: requestId ?? '', expectedHash: expectedHash ?? '' },
    [spaceId, agentId, requestId, expectedHash, purpose],
  );
  const port = useMemo(() => createOwnerPolicyPort(store, target), [store, target]);
  const session = useMemo(() => new OwnerPolicySession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const available = policyAvailability(store, target),
    key = policyScopeKey(available.scope);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active',
        focused = true;
      const activate = () => {
        const scope = port.scope();
        session.activate(foreground && focused && policyScopeKey(scope) === key ? scope : null);
        if (foreground && focused && scope) {
          void session.refresh();
        }
      };
      activate();
      const unsubscribe = store.subscribe(() => {
        if (policyScopeKey(port.scope()) !== key) session.retire();
      });
      const canonical = store.subscribeCanonicalChanges((spaces) => {
        if (!foreground || !focused || (spaces && !spaces.includes(spaceId))) return;
        session.invalidate();
        void session.reconcile();
      });
      const preflight = store.registerSessionPreflight(() => session.retire());
      const cleanup = store.registerSessionCleanup(async () => {
        session.retire();
      });
      const app = AppState.addEventListener('change', (next) => {
        if ((next === 'active') === foreground) return;
        foreground = next === 'active';
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        focused = false;
        app.remove();
        unsubscribe();
        canonical();
        preflight();
        cleanup();
        session.retire();
      };
    }, [store, port, session, key, spaceId]),
  );
  return {
    session,
    unavailableReason: available.reason,
    state: state.scope === key ? state : { ...emptyPolicyState, scope: key },
  };
}
