import { useEffect, useMemo, useSyncExternalStore } from 'react';
import { useClient } from './provider';
import { ConversationSearch } from './conversation-search';
import { conversationScope } from './conversation-scope';

export function useConversationSearch(id: string) {
  const client = useClient();
  const { store } = client;
  const owner = useMemo(
    () => conversationScope(client, store.sessionEpoch, id),
    [client, store, id],
  );
  const search = useMemo(
    () =>
      new ConversationSearch({
        current: () =>
          owner !== null &&
          !store.isSessionRetiring &&
          conversationScope(store.getSnapshot(), store.sessionEpoch, id) === owner,
        search: (query, cursor) =>
          store.driver.request({
            op: 'search_messages',
            space_id: id,
            query,
            cursor,
            limit: 30,
          }),
      }),
    [id, owner, store],
  );
  const searchState = useSyncExternalStore(
    search.subscribe,
    search.getSnapshot,
    search.getSnapshot,
  );
  useEffect(() => {
    // Invalidate at the store notification, even if revoke/restore are batched
    // into a single React render with the same final owner and conversation.
    const changed = () => {
      if (
        !owner ||
        store.isSessionRetiring ||
        conversationScope(store.getSnapshot(), store.sessionEpoch, id) !== owner
      )
        search.close();
    };
    const unsubscribe = store.subscribe(changed);
    const unregister = store.registerSessionCleanup(async () => search.close());
    changed();
    return () => {
      unsubscribe();
      unregister();
      search.close();
    };
  }, [store, id, owner, search]);
  return { owner, search, searchState };
}
