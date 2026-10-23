import { homeOwnerFor } from '../home/access.ts';

type Session = Parameters<typeof homeOwnerFor>[0];
/** One presentation authority key shared by document navigation and export. */
export function documentScope(state: Session, epoch: number): string | null {
  const owner = homeOwnerFor(state);
  return owner ? JSON.stringify([owner.mode, owner.identityId, owner.deviceId, epoch]) : null;
}
