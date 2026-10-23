import type { ClientStore } from './store';

/** Read-only owner of an account surface, including same-account reopen. */
export type AccountScope = Readonly<{
  identity: string;
  device: string;
  relay: string;
  epoch: number;
}>;
export const accountScopeKey = (scope: AccountScope | null): string =>
  scope ? JSON.stringify([scope.identity, scope.device, scope.relay, scope.epoch]) : '';

export function accountScope(store: ClientStore): AccountScope | null {
  const state = store.getSnapshot();
  const account = state.snapshot?.account;
  return state.phase === 'ready' &&
    state.mode === 'account' &&
    !state.erasePending &&
    !store.isSessionRetiring &&
    account?.registered &&
    account.unlocked &&
    !account.setup_pending
    ? {
        identity: account.identity_id,
        device: account.device_id,
        relay: account.relay_url,
        epoch: store.sessionEpoch,
      }
    : null;
}
