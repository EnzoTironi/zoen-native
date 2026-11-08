import { router } from 'expo-router';
import { resolveMessageLink } from '../core/message-link';
import type { ClientStore } from '../core/store';
/** Current Core resolution owns the thread destination, including older or paged-out messages. */
export async function openPinnedMessage(
  store: ClientStore,
  space: string,
  message: string,
  current: () => boolean,
  reveal?: (message: string) => Promise<void>,
) {
  if (!current()) return false;
  const target = await resolveMessageLink(store, space, message, current);
  if (!current() || !target) return false;
  if (reveal && target.pathname === '/chat/[id]') await reveal(target.params.jump);
  else if (reveal) router.push(target);
  else router.replace(target);
  return current();
}
