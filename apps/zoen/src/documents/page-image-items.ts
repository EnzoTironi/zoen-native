import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState } from 'react-native';
import { useFocusEffect, useIsFocused } from 'expo-router';
import type { PageDto } from '../core/types';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { homeSpacesFor } from '../home/access';
import { documentScope } from './document-authority';

const appState = () => AppState.currentState;

/** One canonical read per displayed Page, invalidated before a changed-space refresh. */
export function usePageImageItems(page: PageDto | undefined, enabled: boolean) {
  const state = useClient();
  const { store } = state;
  const focused = useIsFocused();
  const [revision, setRevision] = useState(0);
  const [retired, setRetired] = useState(false);
  const committed = useRef<{ key: string; items: unknown } | null>(null);
  const subscribeAppState = useCallback((listener: () => void) => {
    const subscription = AppState.addEventListener('change', () => {
      committed.current = null;
      setRevision((value) => value + 1);
      listener();
    });
    return () => subscription.remove();
  }, []);
  const foreground = useSyncExternalStore(subscribeAppState, appState, appState) === 'active';
  const space = page?.space_id;
  const scope = documentScope(state, store.sessionEpoch);
  useEffect(
    () =>
      store.subscribeCanonicalChanges((spaces) => {
        if (!spaces || (space && spaces.includes(space))) {
          committed.current = null;
          setRevision((value) => value + 1);
        }
      }),
    [store, space],
  );
  useFocusEffect(
    useCallback(() => {
      setRetired(false);
      setRevision((value) => value + 1);
      if (!scope) return;
      const unregister = store.registerSessionPreflight(() => {
        committed.current = null;
        setRetired(true);
      });
      return () => {
        committed.current = null;
        unregister();
      };
    }, [store, scope]),
  );
  const active =
    !!page &&
    page.ready &&
    enabled &&
    focused &&
    foreground &&
    !retired &&
    !store.isSessionRetiring &&
    !!scope &&
    homeSpacesFor(state).has(page.space_id);
  const key = JSON.stringify([scope, page?.item_id, page?.version, page?.content_hash, revision]);
  const read = useCoreQuery(active ? { op: 'items' } : null, key);
  const lease = useMemo(() => ({ key, items: read.data, active }), [key, read.data, active]);
  useLayoutEffect(() => {
    committed.current = active && read.data ? lease : null;
    return () => {
      if (committed.current === lease) committed.current = null;
    };
  }, [active, read.data, lease]);
  const current = useCallback(() => {
    const now = store.getSnapshot();
    return (
      committed.current === lease &&
      documentScope(now, store.sessionEpoch) === scope &&
      !!space &&
      homeSpacesFor(now).has(space) &&
      !store.isSessionRetiring &&
      AppState.currentState === 'active'
    );
  }, [store, lease, scope, space]);
  return { ...read, active, key, current, items: active ? read.data : null };
}
