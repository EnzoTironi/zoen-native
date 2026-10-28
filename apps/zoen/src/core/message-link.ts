import { conversationScope } from './conversation-scope.ts';
import { messageAddress } from '../notifications/links.ts';
import type { ClientStore } from './store';

export type MessageDestination =
  | { pathname: '/chat/[id]'; params: { id: string; jump: string } }
  | { pathname: '/thread/[space]/[root]'; params: { space: string; root: string; jump: string } };

/** A link never supplies its own thread, read authority or publication operation. */
export async function resolveMessageLink(
  store: ClientStore,
  space: string,
  message: string,
  active: () => boolean,
): Promise<MessageDestination | null> {
  const epoch = store.sessionEpoch;
  const owner = conversationScope(store.getSnapshot(), epoch, space);
  let retired = false;
  const current = () => {
    retired ||=
      !active() ||
      store.isSessionRetiring ||
      store.getSnapshot().mode !== 'account' ||
      !owner ||
      conversationScope(store.getSnapshot(), store.sessionEpoch, space) !== owner;
    return !retired;
  };
  if (!messageAddress(space, message) || !current()) return null;
  const unsubscribe = store.subscribe(current);
  try {
    const entry = await store.driver.request({
      op: 'message',
      space_id: space,
      message_id: message,
    });
    if (!current()) return null;
    if (entry.id !== message || entry.deleted) throw new Error('Message unavailable');
    if (entry.in_thread) {
      if (!messageAddress(space, entry.in_thread)) throw new Error('Message unavailable');
      return {
        pathname: '/thread/[space]/[root]',
        params: { space, root: entry.in_thread, jump: message },
      };
    }
    return { pathname: '/chat/[id]', params: { id: space, jump: message } };
  } finally {
    unsubscribe();
  }
}
