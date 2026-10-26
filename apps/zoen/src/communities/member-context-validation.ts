import type { SpaceMemberContext } from '../core/generated/SpaceMemberContext';
import type { MemberRoleOption } from '../core/generated/MemberRoleOption';
import type { AccountScope } from '../core/account-scope';
const roles = ['admin', 'member', 'reader'];
export const sameMemberRoleOption = (left: MemberRoleOption, right: MemberRoleOption) =>
  left.role === right.role && left.revision === right.revision;
export function validateMemberContext(
  c: SpaceMemberContext,
  space: string,
  member: string,
  owner: AccountScope,
) {
  if (
    !c ||
    c.space_id !== space ||
    c.member_id !== member ||
    c.identity_id !== owner.identity ||
    c.device_id !== owner.device ||
    typeof c.space_title !== 'string' ||
    !/^[a-f0-9]{64}$/.test(c.revision) ||
    !['Active', 'Updating', 'Removing', 'Removed'].includes(c.state) ||
    typeof c.can_remove !== 'boolean' ||
    typeof c.can_add !== 'boolean' ||
    typeof c.can_manage_members !== 'boolean' ||
    typeof c.delivery_ready !== 'boolean' ||
    typeof c.owned_agent !== 'boolean' ||
    typeof c.addition_failed !== 'boolean' ||
    typeof c.removal_failed !== 'boolean' ||
    typeof c.role_change_failed !== 'boolean' ||
    (c.member !== null && c.member.id !== member) ||
    (c.candidate !== null && c.candidate.id !== member) ||
    (c.owned_agent &&
      (!(c.member ?? c.candidate) ||
        (c.member ?? c.candidate)?.kind !== 'Agent' ||
        (c.member ?? c.candidate)?.owner_id !== owner.identity ||
        !(c.member ?? c.candidate)?.is_mine)) ||
    (c.can_add &&
      (!c.can_manage_members ||
        c.state !== 'Removed' ||
        !c.candidate ||
        member === owner.identity ||
        c.member !== null ||
        c.role !== null)) ||
    !Array.isArray(c.role_options) ||
    !c.role_options.every(
      (option) =>
        option &&
        roles.includes(option.role) &&
        option.role !== c.role &&
        /^[a-f0-9]{64}$/.test(option.revision),
    ) ||
    new Set(c.role_options.map((option) => option.role)).size !== c.role_options.length ||
    new Set(c.role_options.map((option) => option.revision)).size !== c.role_options.length ||
    (c.role_options.length > 0 &&
      (!c.can_manage_members ||
        member === owner.identity ||
        c.pending_role !== null ||
        !(
          (c.state === 'Active' && c.member && c.role !== 'owner') ||
          (c.state === 'Removed' && c.can_add && c.candidate)
        ))) ||
    (c.pending_role !== null &&
      (!roles.includes(c.pending_role) ||
        c.pending_role === c.role ||
        c.state !== 'Updating' ||
        !(c.member ?? c.candidate) ||
        c.can_remove ||
        c.can_add ||
        c.role_options.length > 0)) ||
    (c.state === 'Removed' &&
      (c.member !== null ||
        c.role !== null ||
        c.can_remove ||
        c.removal_failed ||
        c.pending_role !== null)) ||
    (c.can_remove && (c.state !== 'Active' || !c.member || member === owner.identity))
  )
    throw Error('Invalid member context');
  return c;
}
