import type { ClientState } from '../core/store';
import type { HomeOwner } from './preferences';

type PresentationSession = Pick<ClientState, 'phase' | 'mode' | 'erasePending' | 'snapshot'>;

export function homeOwnerFor(state: PresentationSession): HomeOwner | null {
  if (state.phase !== 'ready' || state.erasePending || !state.snapshot) return null;
  const account = state.snapshot.account;
  if (state.mode === 'demo')
    return { mode: 'demo', identityId: state.snapshot.me?.id ?? 'demo', deviceId: 'demo' };
  return account?.unlocked && !account.setup_pending
    ? { mode: 'account', identityId: account.identity_id, deviceId: account.device_id }
    : null;
}

/** Cached chats and stale is_me hints never grant access to presentation cards. */
export function homeSpacesFor(state: PresentationSession): ReadonlySet<string> {
  const owner = homeOwnerFor(state);
  const identity = state.snapshot?.me?.id ?? state.snapshot?.account?.identity_id;
  if (!owner || owner.identityId !== identity) return new Set();
  return new Set(
    state.snapshot?.conversations
      .filter((space) => space.members.some((member) => member.id === identity))
      .map((space) => space.id),
  );
}
