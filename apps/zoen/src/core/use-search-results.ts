import { useCallback, useLayoutEffect, useMemo, useSyncExternalStore } from 'react';
import { useFocusEffect } from 'expo-router';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { ConversationSearch, conversationQueryFits } from './conversation-search';
import type { ClientStore } from './store';

export function searchScope(store: ClientStore, space?: string) {
  const state = store.getSnapshot();
  const owner = homeOwnerFor(state);
  const spaces = homeSpacesFor(state);
  return !owner || store.isSessionRetiring || (space && !spaces.has(space))
    ? null
    : JSON.stringify([
        store.sessionEpoch,
        owner,
        state.snapshot?.account?.relay_url ?? null,
        [...spaces].sort(),
        space ?? null,
      ]);
}

/** One disposable canonical result projection for the focused search journey. */
export function useSearchResults(
  store: ClientStore,
  scope: string | null,
  space?: string,
  initial = '',
) {
  const search = useMemo(
    () =>
      new ConversationSearch(
        {
          current: () => scope !== null && searchScope(store, space) === scope,
          fitsQuery: (query) =>
            process.env.EXPO_OS === 'web' ||
            (!space && store.getSnapshot().mode !== 'account') ||
            conversationQueryFits(query),
          search: async (query, cursor) => {
            if (space)
              return store.driver.request({
                op: 'search_messages',
                space_id: space,
                query,
                cursor,
                limit: 30,
              });
            const state = store.getSnapshot(),
              account = state.snapshot?.account;
            if (process.env.EXPO_OS !== 'web' && state.mode === 'account' && account) {
              return store.driver.request({
                op: 'universal_search_page',
                query,
                kinds: [],
                cursor,
                limit: 30,
                expected_identity_id: account.identity_id,
                expected_device_id: account.device_id,
              });
            }
            // Browser and native demo retain their existing bounded search authority.
            const result = await store.driver.request({
              op: 'universal_search',
              query,
              kinds: [],
              limit: 30,
            });
            return { hits: result.hits, next_cursor: null };
          },
        },
        initial,
      ),
    [store, scope, space, initial],
  );
  const state = useSyncExternalStore(search.subscribe, search.getSnapshot, search.getSnapshot);
  useLayoutEffect(() => () => search.close(), [search]);
  useFocusEffect(
    useCallback(() => {
      search.open();
      const stop = store.subscribe(() => {
        if (scope === null || searchScope(store, space) !== scope) search.close();
      });
      return () => {
        stop();
        search.suspend();
      };
    }, [store, scope, space, search]),
  );
  return { search, state };
}
