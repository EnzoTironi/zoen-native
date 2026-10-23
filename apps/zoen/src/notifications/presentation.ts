import type { Snapshot } from '../core/types';
import { localNotes, type LocalNote } from './local';

export type NativeNoteScope = { identityId: string; deviceId: string };
type PresentationPorts = {
  native(scope: NativeNoteScope): Promise<void>;
  local(note: LocalNote): Promise<void>;
};

/** Select one presenter. Native receives scope guards only, never note content. */
export function backgroundPresentationJobs(
  before: Snapshot,
  after: Snapshot,
  native: boolean,
  ports: PresentationPorts,
): Promise<void>[] {
  const notes = localNotes(before, after);
  if (!native) return notes.map((note) => ports.local(note));
  const account = after.account;
  if (
    !notes.length ||
    !account?.unlocked ||
    account.setup_pending ||
    before.account?.identity_id !== account.identity_id ||
    before.account.device_id !== account.device_id
  )
    return [];
  return [ports.native({ identityId: account.identity_id, deviceId: account.device_id })];
}
