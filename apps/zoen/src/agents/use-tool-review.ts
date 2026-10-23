import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import {
  createReviewPort,
  ToolReviewSession,
  reviewScopeKey,
} from '../core/tool-review-renewal.ts';

export function useToolReview(id: string) {
  const { store } = useClient();
  const port = useMemo(() => createReviewPort(store), [store]);
  const session = useMemo(() => new ToolReviewSession(port, id), [port, id]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const key = reviewScopeKey(port.scope());
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const scope = port.scope();
        session.activate(foreground && reviewScopeKey(scope) === key ? scope : null);
        if (foreground) void session.refresh();
      };
      activate();
      const owner = store.subscribe(() => {
        if (reviewScopeKey(port.scope()) !== key) session.retire();
      });
      const canonical = store.subscribeCanonicalChanges(() => {
        void session.refresh();
      });
      const preflight = store.registerSessionPreflight(() => session.retire());
      const app = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (active) activate();
        else session.retire();
      });
      return () => {
        app.remove();
        owner();
        canonical();
        preflight();
        session.retire();
      };
    }, [store, port, session, key]),
  );
  return { session, state };
}
