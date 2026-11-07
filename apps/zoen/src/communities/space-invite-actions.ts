import type { ClientStore } from '../core/store';
import type { SpaceMemberContext } from '../core/generated/SpaceMemberContext';
import type { InviteDto } from '../core/generated/InviteDto';
import { accountScope } from '../core/account-scope.ts';
import { validateMemberContext } from './member-context-validation.ts';

export const inviteDurations = [3600, 86400, 604800, 2592000] as const;
export const inviteUses = [1, 10, 50, 100, 1000, 10000] as const;
export type InviteFlight = { key: object };
type InviteAction = {
  store: ClientStore;
  context: SpaceMemberContext;
  key: object;
  flight: { current: InviteFlight | null };
  current(): boolean;
  pending(value: boolean): void;
  failed(): void;
};
async function run(action: InviteAction, work: () => Promise<void>) {
  if (!action.current() || action.flight.current?.key === action.key) return;
  const attempt = { key: action.key };
  action.flight.current = attempt;
  action.pending(true);
  try {
    await work();
  } catch {
    if (action.current()) action.failed();
  } finally {
    if (action.flight.current === attempt) {
      action.flight.current = null;
      if (action.current()) action.pending(false);
    }
  }
}
/** Core rechecks the native review; the relay owns the actual invitation. */
export function createSpaceInvite(
  action: InviteAction & { ttl: number; uses: number; created(value: InviteDto): void },
) {
  return run(action, async () => {
    const { context, store } = action;
    const value = await store.driver.request({
      op: 'create_invite_reviewed',
      input: {
        space_id: context.space_id,
        expected_identity_id: context.identity_id,
        expected_device_id: context.device_id,
        observed_revision: context.revision,
        ttl_secs: action.ttl,
        max_uses: action.uses,
      },
    });
    if (action.current()) action.created(value);
  });
}
/** Re-read permissions before handing a link to a system share/clipboard surface. */
export function discloseSpaceInvite(action: InviteAction & { disclose(): Promise<unknown> }) {
  return run(action, async () => {
    const { store, context } = action;
    const owner = accountScope(store);
    if (!owner) throw Error('Invite owner unavailable');
    const fresh = validateMemberContext(
      await store.driver.request({
        op: 'space_member_context',
        space_id: context.space_id,
        identity_id: context.identity_id,
        expected_identity_id: context.identity_id,
        expected_device_id: context.device_id,
      }),
      context.space_id,
      context.identity_id,
      owner,
    );
    if (!action.current()) return;
    if (
      fresh.revision !== context.revision ||
      !fresh.can_manage_members ||
      !fresh.delivery_ready ||
      fresh.state !== 'Active'
    )
      throw Error('Invite permission changed');
    await action.disclose();
  });
}
