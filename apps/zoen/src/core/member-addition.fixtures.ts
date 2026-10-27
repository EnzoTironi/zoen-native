import type { MemberRoleOption } from './generated/MemberRoleOption';
import type { SpaceMemberContext } from './generated/SpaceMemberContext';
import { context, scope, peer, settle } from './community-members.fixtures.ts';
import type { AccountScope } from './account-scope';
import { MemberAdditionSession } from '../communities/member-addition.ts';
import type { MemberAdditionPort } from '../communities/member-context-port';
export { scope, peer, settle };
export function additionContext(): SpaceMemberContext {
  const current = context();
  return {
    ...current,
    state: 'Removed',
    role: null,
    member: null,
    can_remove: false,
    can_add: true,
    role_options: [
      { role: 'member', revision: 'c'.repeat(64) },
      { role: 'reader', revision: 'a'.repeat(64) },
      { role: 'admin', revision: 'b'.repeat(64) },
    ],
    delivery_ready: false,
    candidate: { ...current.member!, kind: 'Agent' },
  };
}
export async function additionFixture() {
  let owner: AccountScope | null = { ...scope },
    value = additionContext();
  let confirm: (review: SpaceMemberContext) => Promise<boolean> = async () => true;
  let write: MemberAdditionPort['setRole'] = async (review, option) => {
    value = {
      ...review,
      can_add: false,
      role_options: [],
      pending_role: option.role,
      state: 'Updating',
      member: null,
      role: null,
    };
    return value;
  };
  const choices: MemberRoleOption[] = [];
  const writes: SpaceMemberContext[] = [],
    reads: string[] = [],
    searches: string[] = [];
  let read: MemberAdditionPort['read'] = async (id) => ({
    ...value,
    member_id: id,
    state: id === peer ? value.state : 'Active',
    member: id === peer ? value.member : context(id).member,
    role: id === peer ? value.role : 'owner',
    pending_role: id === peer ? value.pending_role : null,
    candidate: id === peer ? value.candidate : null,
    can_add: id === peer && value.can_add,
    role_options: id === peer ? value.role_options : [],
  });
  let find: MemberAdditionPort['find'] = async () => [value];
  const port: MemberAdditionPort = {
    scope: () => owner,
    read: (id, scope) => {
      reads.push(id);
      return read(id, scope);
    },
    find: (query, scope) => {
      searches.push(query);
      return find(query, scope);
    },
    setRole: (review, option, scope) => {
      writes.push(review);
      choices.push(option);
      return write(review, option, scope);
    },
  };
  const session = new MemberAdditionSession('sp_fixture', port);
  session.activate(owner);
  await settle();
  return {
    session,
    port,
    writes,
    choices,
    reads,
    searches,
    async open() {
      session.open();
      await settle();
    },
    select(ticket = session.ticket(), option?: MemberRoleOption) {
      return session.select(peer, confirm, ticket, option);
    },
    confirm(next: typeof confirm) {
      confirm = next;
    },
    scope(next: AccountScope | null) {
      owner = next;
    },
    value(next: SpaceMemberContext) {
      value = next;
    },
    read(next: typeof read) {
      read = next;
    },
    find(next: typeof find) {
      find = next;
    },
    write(next: typeof write) {
      write = next;
    },
  };
}
