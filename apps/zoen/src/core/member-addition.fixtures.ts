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
    candidate: { ...current.member!, kind: 'Agent' },
  };
}
export async function additionFixture() {
  let owner: AccountScope | null = { ...scope },
    value = additionContext();
  let confirm: (review: SpaceMemberContext) => Promise<boolean> = async () => true;
  let write: MemberAdditionPort['add'] = async (review) => {
    value = { ...review, can_add: false, state: 'Updating', member: review.candidate };
    return value;
  };
  const writes: SpaceMemberContext[] = [],
    reads: string[] = [],
    searches: string[] = [];
  let read: MemberAdditionPort['read'] = async (id) => ({
    ...value,
    member_id: id,
    candidate: id === peer ? value.candidate : null,
    can_add: id === peer && value.can_add,
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
    add: (review, scope) => {
      writes.push(review);
      return write(review, scope);
    },
  };
  const session = new MemberAdditionSession('sp_fixture', port);
  session.activate(owner);
  await settle();
  return {
    session,
    port,
    writes,
    reads,
    searches,
    async open() {
      session.open();
      await settle();
    },
    select(ticket = session.ticket()) {
      return session.select(peer, confirm, ticket);
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
