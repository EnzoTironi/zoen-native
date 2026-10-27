import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import type { ClientStore } from '../core/store';
import { validLocalNotification } from './links.ts';

/** OS presentation asks Core now; cached mute flags may have expired. */
export async function allowLocalNotification(
  store: ClientStore,
  data: unknown,
  deviceId: string,
): Promise<boolean> {
  const owner = accountScope(store);
  const target = validLocalNotification(data, deviceId);
  if (!owner || owner.device !== deviceId || !target) return false;
  const current = () =>
    accountScopeKey(accountScope(store)) === accountScopeKey(owner) &&
    store
      .getSnapshot()
      .snapshot?.conversations.some(
        (chat) => chat.id === target.spaceId && chat.members.some((m) => m.id === owner.identity),
      );
  if (!current()) return false;
  const snapshot = await store.driver.request({ op: 'snapshot' });
  if (!current()) return false;
  const account = snapshot.account;
  const chat = snapshot.conversations.find((row) => row.id === target.spaceId);
  return Boolean(
    account?.registered &&
    account.unlocked &&
    !account.setup_pending &&
    account.identity_id === owner.identity &&
    account.device_id === owner.device &&
    account.relay_url === owner.relay &&
    chat?.members.some((m) => m.id === owner.identity) &&
    !chat.muted &&
    !chat.archived,
  );
}
