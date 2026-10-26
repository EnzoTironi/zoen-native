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
  reviewStatusEligible,
} from '../core/tool-review-renewal.ts';

export function useToolReview(id: string, onExpiry?: () => void) {
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
      let notifiedExpiry = '';
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
          !reviewStatusEligible(current.request?.status) ||
          !tool ||
          !toolApprovalValid(tool)
        )
          return;
        const expiry = Number(tool.expires_ms);
        const remaining = expiry - Date.now();
        if (!Number.isSafeInteger(expiry)) return;
        const notify = () => {
          const observation = JSON.stringify([
            current.request?.id,
            tool.content_hash,
            tool.expires_ms,
          ]);
          if (notifiedExpiry === observation) return;
          notifiedExpiry = observation;
          // Presentation only: recompute the parent's clock-derived affordance.
          // This never supplies a DTO, permission, intent or execution result.
          onExpiry?.();
        };
        if (remaining <= 0) {
          notify();
          return;
        }
        expiryTimer = setTimeout(
          () => {
            expiryTimer = undefined;
            if (!foreground || reviewScopeKey(port.scope()) !== key) return;
            if (Date.now() >= expiry) {
              notify();
              void session.refresh();
            } else scheduleExpiry();
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
    }, [store, port, session, key, onExpiry]),
  );
  return { session, state };
}
