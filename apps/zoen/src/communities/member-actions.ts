import type { SpaceMemberContext } from '../core/generated/SpaceMemberContext';
import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { MemberActionsPort } from './member-context-port.ts';

export type MemberActionTicket = Readonly<{ scope: string; generation: number; member: string }>;
export type MemberActionsState = Readonly<{
  scope: string;
  selected: string | null;
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  context: SpaceMemberContext | null;
  busy: boolean;
  actionFailed: boolean;
}>;
export const emptyMemberActions: MemberActionsState = {
  scope: '',
  selected: null,
  phase: 'inactive',
  context: null,
  busy: false,
  actionFailed: false,
};
function validate(context: SpaceMemberContext, space: string, member: string, owner: AccountScope) {
  if (
    !context ||
    context.space_id !== space ||
    context.member_id !== member ||
    context.identity_id !== owner.identity ||
    context.device_id !== owner.device ||
    typeof context.space_title !== 'string' ||
    !/^[a-f0-9]{64}$/.test(context.revision) ||
    !['Active', 'Updating', 'Removing', 'Removed'].includes(context.state) ||
    typeof context.can_remove !== 'boolean' ||
    typeof context.removal_failed !== 'boolean' ||
    (context.member !== null && context.member.id !== member) ||
    (context.state === 'Removed' &&
      (context.member !== null || context.can_remove || context.removal_failed)) ||
    (context.can_remove &&
      (context.state !== 'Active' || !context.member || member === owner.identity))
  )
    throw Error('Invalid member context');
  return context;
}

/** Focus-owned projection. Only Core's signed log and existing outbox persist work. */
export class MemberActionsSession {
  private state = emptyMemberActions;
  private owner: AccountScope | null = null;
  private generation = 0;
  private readGeneration = 0;
  private review: object | null = null;
  // Reconciliation/foreground cannot clear a command already in flight.
  private readonly writes = new Map<string, object>();
  private readonly listeners = new Set<() => void>();
  private readonly space: string;
  private readonly port: MemberActionsPort;
  constructor(space: string, port: MemberActionsPort) {
    this.space = space;
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<MemberActionsState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  activate(owner: AccountScope | null) {
    ++this.generation;
    ++this.readGeneration;
    this.owner = owner ? Object.freeze({ ...owner }) : null;
    this.review = null;
    this.state = { ...emptyMemberActions, scope: accountScopeKey(owner) };
    this.publish({ busy: this.writes.has(this.state.scope) });
  }
  retire() {
    this.activate(null);
  }
  ticket(): MemberActionTicket | null {
    return this.owner &&
      this.state.selected &&
      accountScopeKey(this.owner) === accountScopeKey(this.port.scope())
      ? Object.freeze({
          scope: this.state.scope,
          generation: this.generation,
          member: this.state.selected,
        })
      : null;
  }
  private current(ticket: MemberActionTicket | null) {
    return Boolean(
      ticket &&
      this.owner &&
      ticket.scope === this.state.scope &&
      ticket.generation === this.generation &&
      ticket.member === this.state.selected &&
      ticket.scope === accountScopeKey(this.port.scope()),
    );
  }
  open(member: string) {
    if (!this.owner || accountScopeKey(this.owner) !== accountScopeKey(this.port.scope())) return;
    ++this.generation;
    this.review = null;
    this.publish({
      selected: member,
      context: null,
      phase: 'loading',
      actionFailed: false,
      busy: this.writes.has(this.state.scope),
    });
    void this.refresh(this.ticket());
  }
  close(ticket?: MemberActionTicket | null) {
    if (ticket !== undefined && !this.current(ticket)) return;
    ++this.generation;
    ++this.readGeneration;
    this.review = null;
    this.publish({
      selected: null,
      context: null,
      phase: 'inactive',
      busy: this.writes.has(this.state.scope),
    });
  }
  canOpenProfile(ticket: MemberActionTicket | null): boolean {
    return Boolean(
      this.current(ticket) && this.state.phase === 'ready' && this.state.context?.member,
    );
  }
  async refresh(ticket: MemberActionTicket | null, failed = false) {
    if (!this.current(ticket) || !this.owner || !ticket) return;
    const owner = this.owner;
    const read = ++this.readGeneration;
    this.publish({ phase: 'loading' });
    try {
      const context = validate(
        await this.port.read(ticket.member, owner),
        this.space,
        ticket.member,
        owner,
      );
      if (!this.current(ticket) || read !== this.readGeneration) return;
      if (context.state === 'Removed') {
        this.close();
        return;
      }
      this.publish({
        phase: 'ready',
        context,
        busy: Boolean(this.review || this.writes.has(ticket.scope)),
        actionFailed: context.removal_failed || (failed && context.state !== 'Removing'),
      });
    } catch {
      if (this.current(ticket) && read === this.readGeneration)
        this.publish({ phase: 'failed', context: null, actionFailed: failed });
    }
  }
  async remove(
    confirm: (context: SpaceMemberContext) => Promise<boolean>,
    ticket: MemberActionTicket | null,
  ) {
    const context = this.state.context;
    if (
      !this.current(ticket) ||
      !ticket ||
      !this.owner ||
      this.state.phase !== 'ready' ||
      !context?.can_remove ||
      this.state.busy ||
      this.review ||
      this.writes.has(ticket.scope)
    )
      return;
    const owner = this.owner;
    const review = {};
    this.review = review;
    this.publish({ busy: true, actionFailed: false });
    let dispatched = false;
    let failed = false;
    try {
      if (!(await confirm(context)) || !this.current(ticket) || this.review !== review) return;
      const current = validate(
        await this.port.read(ticket.member, owner),
        this.space,
        ticket.member,
        owner,
      );
      if (!this.current(ticket) || this.review !== review) return;
      if (!current.can_remove || current.revision !== context.revision) {
        await this.refresh(ticket);
        return;
      }
      this.writes.set(ticket.scope, review);
      dispatched = true;
      await this.port.remove(current, owner);
    } catch {
      failed = true;
    } finally {
      if (this.review === review) this.review = null;
      if (dispatched && this.writes.get(ticket.scope) === review) this.writes.delete(ticket.scope);
      const next = dispatched ? this.ticket() : ticket;
      if (this.current(next) && next?.scope === ticket.scope) {
        this.publish({ busy: Boolean(this.review || this.writes.has(ticket.scope)) });
        if (dispatched || failed) await this.refresh(next, failed && next.member === ticket.member);
      }
    }
  }
}
