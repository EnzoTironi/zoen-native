import { router } from 'expo-router';
import { conversationScope } from '../core/conversation-scope';
import { homeOwnerFor } from '../home/access';
import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';

/** Capture the session at render; a held menu action cannot borrow a later same-ID login. */
export function messageInfoAction(
  store: ClientStore,
  spaceId: string,
  entry: TimelineEntry,
  current: (entry: TimelineEntry) => TimelineEntry | undefined,
  epoch = store.sessionEpoch,
): () => void {
  return () => {
    const state = store.getSnapshot();
    const owner = homeOwnerFor(state);
    const scope = conversationScope(state, store.sessionEpoch, spaceId);
    if (
      store.isSessionRetiring ||
      epoch !== store.sessionEpoch ||
      owner?.mode !== 'account' ||
      !scope ||
      !current(entry)
    )
      return;
    router.push({
      pathname: '/message-info/[space]/[message]',
      params: { space: spaceId, message: entry.id, owner: scope },
    });
  };
}
