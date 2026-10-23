import type { ClientStore } from '../core/store';
import type { ProfileDto } from '../core/generated/ProfileDto';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';

export type BlockedPeopleScope = AccountScope;
export type BlockedPerson = Readonly<{
  id: string;
  profile: ProfileDto | null;
  phase: 'idle' | 'loading' | 'ready' | 'failed';
}>;
export type BlockedPeopleState = Readonly<{
  scope: string;
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  people: readonly BlockedPerson[];
  pending: string | null;
  readFailed: boolean;
  actionFailed: boolean;
}>;
export interface BlockedPeoplePort {
  scope(): BlockedPeopleScope | null;
  blocked(): Promise<string[]>;
  profile(id: string): Promise<ProfileDto>;
  unblock(id: string): Promise<unknown>;
}
export type BlockedPeopleTicket = Readonly<{ scope: string; revision: number }>;
export const emptyBlockedPeople: BlockedPeopleState = {
  scope: '',
  phase: 'inactive',
  people: [],
  pending: null,
  readFailed: false,
  actionFailed: false,
};
export const blockedPeopleScopeKey = accountScopeKey;

export function createBlockedPeoplePort(store: ClientStore): BlockedPeoplePort {
  return {
    scope: () => accountScope(store),
    blocked: () => store.driver.request({ op: 'blocked_people' }),
    profile: (identity_id) => store.driver.request({ op: 'profile', identity_id }),
    unblock: (identity_id) => store.request({ op: 'unblock_person', identity_id }),
  };
}

/** A focus-scoped projection; Core alone persists the blocked list. */
export class BlockedPeopleSession {
  private state = emptyBlockedPeople;
  private owner: BlockedPeopleScope | null = null;
  private revision = 0;
  private readonly listeners = new Set<() => void>();
  private readonly port: BlockedPeoplePort;
  constructor(port: BlockedPeoplePort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (callback: () => void) => {
    this.listeners.add(callback);
    return () => {
      this.listeners.delete(callback);
    };
  };
  private publish(patch: Partial<BlockedPeopleState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  activate(scope: BlockedPeopleScope | null) {
    this.owner = scope ? Object.freeze({ ...scope }) : null;
    ++this.revision;
    this.state = { ...emptyBlockedPeople, scope: blockedPeopleScopeKey(scope) };
    this.publish({});
  }
  retire() {
    this.activate(null);
  }
  private current(revision = this.revision) {
    return Boolean(
      this.owner &&
      revision === this.revision &&
      blockedPeopleScopeKey(this.owner) === blockedPeopleScopeKey(this.port.scope()),
    );
  }
  ticket(): BlockedPeopleTicket | null {
    return this.current()
      ? Object.freeze({ scope: this.state.scope, revision: this.revision })
      : null;
  }
  private accepts(ticket: BlockedPeopleTicket | null) {
    return Boolean(ticket && ticket.scope === this.state.scope && this.current(ticket.revision));
  }
  async load(ticket: BlockedPeopleTicket | null, preserveActionFailure = false) {
    if (!this.accepts(ticket)) return;
    const revision = ++this.revision;
    this.publish({
      phase: 'loading',
      pending: null,
      readFailed: false,
      actionFailed: preserveActionFailure && this.state.actionFailed,
    });
    try {
      const ids = await this.port.blocked();
      if (!this.current(revision)) return;
      if (ids.some((id) => typeof id !== 'string' || !id) || new Set(ids).size !== ids.length)
        throw new Error('invalid list');
      this.publish({
        phase: 'ready',
        people: ids.map((id) => ({ id, profile: null, phase: 'idle' })),
      });
    } catch {
      if (this.current(revision)) this.publish({ phase: 'failed', readFailed: true });
    }
  }
  async loadProfile(id: string, ticket: BlockedPeopleTicket | null) {
    const person = this.state.people.find((entry) => entry.id === id);
    if (
      !this.accepts(ticket) ||
      this.state.phase !== 'ready' ||
      !person ||
      person.phase === 'ready' ||
      person.phase === 'loading'
    )
      return;
    const revision = this.revision;
    const update = (patch: Partial<BlockedPerson>) =>
      this.publish({
        people: this.state.people.map((entry) =>
          entry.id === id ? { ...entry, ...patch } : entry,
        ),
      });
    update({ phase: 'loading' });
    try {
      const profile = await this.port.profile(id);
      if (!this.current(revision)) return;
      if (profile.identity_id !== id || profile.is_me) throw new Error('foreign profile');
      update({ profile, phase: 'ready' });
    } catch {
      if (this.current(revision)) update({ profile: null, phase: 'failed' });
    }
  }
  canOpen(id: string, ticket: BlockedPeopleTicket | null): boolean {
    return Boolean(
      this.accepts(ticket) &&
      this.state.phase === 'ready' &&
      !this.state.pending &&
      this.state.people.some((person) => person.id === id && person.phase === 'ready'),
    );
  }
  async unblock(id: string, confirm: () => Promise<boolean>, ticket: BlockedPeopleTicket | null) {
    if (
      !this.accepts(ticket) ||
      this.state.phase !== 'ready' ||
      this.state.pending ||
      !this.state.people.some((person) => person.id === id)
    )
      return;
    const revision = this.revision;
    this.publish({ pending: id, actionFailed: false });
    try {
      const accepted = await confirm();
      if (!accepted || !this.current(revision)) return;
      await this.port.unblock(id);
      if (this.current(revision)) await this.load(this.ticket());
    } catch {
      if (this.current(revision)) {
        this.publish({ actionFailed: true });
        // An error can follow a local commit. Read Core; never retry the mutation.
        await this.load(this.ticket(), true);
      }
    } finally {
      if (this.current(revision)) this.publish({ pending: null });
    }
  }
}
