import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { toolApprovalValid } from '../core/tool-request-decision';
import { useClient } from '../core/provider';
import { accountScope } from '../core/account-scope';
import {
  createReviewPort,
  ToolReviewSession,
  reviewScopeKey,
} from '../core/tool-review-renewal.ts';

export function useToolReview(id: string) {
  const client = useClient();
  const { store } = client;
  const port = useMemo(() => createReviewPort(store), [store]);
  const session = useMemo(() => new ToolReviewSession(port, id), [port, id]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const key = reviewScopeKey(accountScope(store, client));
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      let expiryTimer: ReturnType<typeof setTimeout> | undefined;
      const clearExpiry = () => {
        clearTimeout(expiryTimer);
        expiryTimer = undefined;
      };
      const scheduleExpiry = () => {
        clearExpiry();
        const current = session.getSnapshot();
        const tool = current.request?.tool_approval;
        if (
          !foreground ||
          reviewScopeKey(port.scope()) !== key ||
          current.phase !== 'ready' ||
          current.request?.status !== 'Approved' ||
          !tool ||
          !toolApprovalValid(tool)
        )
          return;
        const expiry = Number(tool.expires_ms);
        const remaining = expiry - Date.now();
        if (!Number.isSafeInteger(expiry) || remaining <= 0) return;
        expiryTimer = setTimeout(
          () => {
            expiryTimer = undefined;
            if (!foreground || reviewScopeKey(port.scope()) !== key) return;
            if (Date.now() >= expiry) void session.refresh();
            else scheduleExpiry();
          },
          Math.min(remaining, 2_147_483_647),
        );
      };
      const expiry = session.subscribe(scheduleExpiry);
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
        expiry();
        clearExpiry();
        session.retire();
      };
    }, [store, port, session, key]),
  );
  return { session, state };
}
