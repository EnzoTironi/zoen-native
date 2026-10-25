import type { SpaceMemberContext } from './generated/SpaceMemberContext';
import type { AccountScope } from './account-scope';
import { MemberActionsSession } from '../communities/member-actions.ts';
import type { MemberActionsPort } from '../communities/member-context-port';

export const scope: AccountScope = {
  identity: 'a'.repeat(64),
  device: 'd'.repeat(64),
  relay: 'https://relay.invalid/',
  epoch: 1,
};
export const peer = 'b'.repeat(64);
export function context(member = peer): SpaceMemberContext {
  return {
    identity_id: scope.identity,
    device_id: scope.device,
    space_id: 'sp_fixture',
    space_title: 'Community',
    member_id: member,
    role: 'member',
    state: 'Active',
    can_remove: true,
    can_manage_members: true,
    can_add: false,
    candidate: null,
    addition_failed: false,
    removal_failed: false,
    revision: '1'.repeat(64),
    member: {
      id: member,
      kind: 'Person',
      name: member === peer ? 'Selected' : 'Other',
      handle: 'selected',
      initials: 'S',
      tint_hex: '#246844',
      glyph: null,
      bio: '',
      owner_id: null,
      owner_name: null,
      owner_tint_hex: null,
      photo_sha256: null,
      photo_ready: false,
      photo_mime: null,
      is_me: false,
      is_mine: false,
    },
  };
}
export function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export async function settle() {
  for (let n = 0; n < 30; ++n) await Promise.resolve();
}
export function fixture() {
  let owner: AccountScope | null = { ...scope };
  let value = context();
  let reader: MemberActionsPort['read'] = async (id) => ({
    ...value,
    member_id: id,
    member: value.member ? { ...value.member, id } : null,
  });
  let writer: MemberActionsPort['remove'] = async (review) => {
    value = { ...review, state: 'Removing', can_remove: false };
    return value;
  };
  const reads: { id: string; owner: AccountScope }[] = [];
  const writes: { review: SpaceMemberContext; owner: AccountScope }[] = [];
  const port: MemberActionsPort = {
    scope: () => owner,
    read: (id, current) => {
      reads.push({ id, owner: current });
      return reader(id, current);
    },
    remove: (review, current) => {
      writes.push({ review, owner: current });
      return writer(review, current);
    },
  };
  const session = new MemberActionsSession('sp_fixture', port);
  session.activate(scope);
  return {
    session,
    port,
    reads,
    writes,
    scope(next: AccountScope | null) {
      owner = next;
    },
    value(next: SpaceMemberContext) {
      value = next;
    },
    read(next: MemberActionsPort['read']) {
      reader = next;
    },
    write(next: MemberActionsPort['remove']) {
      writer = next;
    },
    async open(id = peer) {
      session.open(id);
      await settle();
    },
    remove(answer: (value: SpaceMemberContext) => Promise<boolean> = async () => true) {
      return session.remove(answer, session.ticket());
    },
  };
}
