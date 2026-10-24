import type { ClientStore } from '../core/store';
import type { SpaceMemberContext } from '../core/generated/SpaceMemberContext';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';

export interface MemberActionsPort {
  scope(): AccountScope | null;
  read(member: string, owner: AccountScope): Promise<SpaceMemberContext>;
  remove(context: SpaceMemberContext, owner: AccountScope): Promise<SpaceMemberContext>;
}
export function createMemberActionsPort(store: ClientStore, space: string): MemberActionsPort {
  const check = (owner: AccountScope) => {
    if (accountScopeKey(owner) !== accountScopeKey(accountScope(store)))
      throw Error('Member action account changed');
  };
  return {
    scope: () => accountScope(store),
    read: (identity_id, owner) => {
      check(owner);
      return store.driver.request({
        op: 'space_member_context',
        space_id: space,
        identity_id,
        expected_identity_id: owner.identity,
        expected_device_id: owner.device,
      });
    },
    remove: (context, owner) => {
      check(owner);
      return store.request({
        op: 'remove_member_reviewed',
        space_id: space,
        identity_id: context.member_id,
        expected_identity_id: owner.identity,
        expected_device_id: owner.device,
        observed_revision: context.revision,
      });
    },
  };
}
