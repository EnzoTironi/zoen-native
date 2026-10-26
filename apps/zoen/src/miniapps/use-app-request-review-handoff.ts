import { useCallback, useEffect, useRef, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import type { ClientStore } from '../core/store';
import { resolveAppRequestReview, type AppRequestReviewTarget } from './app-request-review';
import type { EditorAgentSession } from './editor-agent-session';

/** Consume a route intent once; cancellation/failed reads never reopen a sheet. */
export function useAppRequestReviewHandoff({
  target,
  ready,
  store,
  itemId,
  session,
  onConsumed,
  onFailure,
  onOpen,
}: {
  target: AppRequestReviewTarget | null;
  ready: boolean;
  store: ClientStore;
  itemId: string;
  session: EditorAgentSession;
  onConsumed?: () => void;
  onFailure?: () => void;
  onOpen: () => void;
}) {
  const [intent, setIntent] = useState<AppRequestReviewTarget | null>(null);
  const consumed = useRef('');
  const started = useRef('');
  const focused = useRef(false);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
        session.close();
      };
    }, [session]),
  );
  const key = target ? JSON.stringify(target) : '';
  useEffect(() => {
    if (!target || consumed.current === key) return;
    consumed.current = key;
    setIntent(target);
    onConsumed?.();
  }, [target, key, onConsumed]);
  useEffect(() => {
    if (!intent || !ready) return;
    const intentKey = JSON.stringify(intent);
    if (started.current === intentKey) return;
    started.current = intentKey;
    let live = true;
    const check = () => {
      if (!live || !focused.current) throw Error('app_review_retired');
    };
    void resolveAppRequestReview(store, intent, itemId, check)
      .then(
        () => {
          check();
          onOpen();
          return session.openReview(intent, check);
        },
        () => false,
      )
      .then(
        (opened) => {
          if (live && focused.current && !opened) onFailure?.();
        },
        () => {
          if (live && focused.current) onFailure?.();
        },
      );
    return () => {
      live = false;
    };
  }, [intent, ready, store, itemId, session, onFailure, onOpen]);
}
