import type { ClientState } from '../core/store';
import { validSurfaceOwner, type SurfaceOwner } from './snapshot.ts';

type SurfaceSession = Pick<ClientState, 'phase' | 'mode' | 'erasePending' | 'snapshot'>;

/** Cached projections are usable only by the current unlocked account/device. */
export function surfaceOwner(state: SurfaceSession): SurfaceOwner | null {
  const account = state.snapshot?.account;
  if (
    state.phase !== 'ready' ||
    state.mode !== 'account' ||
    state.erasePending ||
    !account?.unlocked ||
    account.setup_pending
  )
    return null;
  const owner = { identityId: account.identity_id, deviceId: account.device_id };
  return validSurfaceOwner(owner) ? owner : null;
}

/** A cached conversation is not an access grant after the current identity leaves it. */
export function surfaceSpaces(state: SurfaceSession): ReadonlyMap<string, string> {
  const owner = surfaceOwner(state);
  const snapshot = state.snapshot;
  const identityId = snapshot?.me?.id ?? snapshot?.account?.identity_id;
  if (!snapshot || !owner || identityId !== owner.identityId) return new Map();
  return new Map(
    snapshot.conversations
      .filter((space) => space.members.some((member) => member.id === identityId))
      .map((space) => [space.id, space.title]),
  );
}
