import type { SpaceMemberContext } from '../core/generated/SpaceMemberContext';
import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { MemberAdditionPort } from './member-context-port.ts';

export type AdditionTicket = Readonly<{ scope: string; generation: number }>;
export type MemberAdditionState = Readonly<{
  scope: string;
  open: boolean;
  canManage: boolean;
  query: string;
  phase: 'idle' | 'loading' | 'ready' | 'failed';
  results: SpaceMemberContext[];
  selected: SpaceMemberContext | null;
  busy: boolean;
  failed: boolean;
}>;
export const emptyMemberAddition: MemberAdditionState = {
  scope: '',
  open: false,
  canManage: false,
  query: '',
  phase: 'idle',
  results: [],
  selected: null,
  busy: false,
  failed: false,
};
function validate(c: SpaceMemberContext, space: string, owner: AccountScope, id?: string) {
  if (
    !c ||
    c.space_id !== space ||
    (id !== undefined && c.member_id !== id) ||
    c.identity_id !== owner.identity ||
    c.device_id !== owner.device ||
    !/^[a-f0-9]{64}$/.test(c.revision) ||
    typeof c.can_manage_members !== 'boolean' ||
    typeof c.can_add !== 'boolean' ||
    typeof c.addition_failed !== 'boolean' ||
    !['Active', 'Updating', 'Removing', 'Removed'].includes(c.state) ||
    (c.candidate !== null && c.candidate.id !== c.member_id) ||
    (c.can_add &&
      (!c.can_manage_members ||
        c.state !== 'Removed' ||
        !c.candidate ||
        c.member_id === owner.identity ||
        c.member !== null ||
        c.role !== null))
  )
    throw Error('Invalid member addition context');
  return c;
}

/** Focus-owned review; membership and pending/failure durability remain entirely in Core. */
export class MemberAdditionSession {
  private state = emptyMemberAddition;
  private owner: AccountScope | null = null;
  private generation = 0;
  private reading = 0;
  private review: object | null = null;
  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly writes = new Map<string, object>();
  private readonly listeners = new Set<() => void>();
  private readonly space: string;
  private readonly port: MemberAdditionPort;
  constructor(space: string, port: MemberAdditionPort) {
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
  private publish(patch: Partial<MemberAdditionState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  activate(owner: AccountScope | null) {
    if (this.searchTimer) clearTimeout(this.searchTimer);
    this.searchTimer = null;
    ++this.generation;
    ++this.reading;
    this.review = null;
    this.owner = owner ? Object.freeze({ ...owner }) : null;
    this.state = { ...emptyMemberAddition, scope: accountScopeKey(owner) };
    this.publish({ busy: this.writes.has(this.state.scope) });
    void this.refresh();
  }
  retire() {
    this.activate(null);
  }
  ticket(observed = this.state): AdditionTicket | null {
    return observed === this.state &&
      this.owner &&
      this.state.scope === accountScopeKey(this.port.scope())
      ? Object.freeze({ scope: this.state.scope, generation: this.generation })
      : null;
  }
  private current(ticket: AdditionTicket | null) {
    return Boolean(
      ticket &&
      this.owner &&
      ticket.scope === this.state.scope &&
      ticket.generation === this.generation &&
      ticket.scope === accountScopeKey(this.port.scope()),
    );
  }
  open(ticket: AdditionTicket | null = this.ticket()) {
    if (!this.current(ticket) || !this.state.canManage) return;
    ++this.generation;
    this.publish({ open: true, query: '', selected: null, results: [], failed: false });
    void this.refresh();
  }
  close(ticket: AdditionTicket | null) {
    if (!this.current(ticket)) return;
    if (this.searchTimer) clearTimeout(this.searchTimer);
    this.searchTimer = null;
    ++this.generation;
    ++this.reading;
    this.review = null;
    this.publish({
      open: false,
      selected: null,
      results: [],
      query: '',
      phase: 'idle',
      busy: this.writes.has(this.state.scope),
    });
  }
  search(query: string, ticket: AdditionTicket | null) {
    if (!this.current(ticket) || !this.state.open || this.state.busy) return;
    this.publish({ query, selected: null, failed: false });
    ++this.reading;
    if (this.searchTimer) clearTimeout(this.searchTimer);
    this.searchTimer = setTimeout(() => {
      this.searchTimer = null;
      if (this.current(ticket)) void this.refresh();
    }, 250);
  }
  async refresh() {
    const ticket = this.ticket(),
      owner = this.owner;
    if (!this.current(ticket) || !owner) return;
    const reading = ++this.reading;
    if (this.state.open) this.publish({ phase: 'loading' });
    try {
      const permission = validate(
        await this.port.read(owner.identity, owner),
        this.space,
        owner,
        owner.identity,
      );
      if (!this.current(ticket) || reading !== this.reading) return;
      if (!permission.can_manage_members) {
        this.publish({ canManage: false, results: [], selected: null, phase: 'ready' });
        return;
      }
      this.publish({ canManage: true });
      if (!this.state.open) return;
      const results = (await this.port.find(this.state.query, owner)).map((c) =>
        validate(c, this.space, owner),
      );
      if (!this.current(ticket) || reading !== this.reading) return;
      const selected = this.state.selected
        ? (results.find((c) => c.member_id === this.state.selected?.member_id) ??
          this.state.selected)
        : null;
      // Read the selected member even when the search list no longer contains it.
      const current = selected
        ? validate(
            await this.port.read(selected.member_id, owner),
            this.space,
            owner,
            selected.member_id,
          )
        : null;
      if (!this.current(ticket) || reading !== this.reading) return;
      this.publish({ results, selected: current, phase: 'ready' });
    } catch {
      if (this.current(ticket) && reading === this.reading) this.publish({ phase: 'failed' });
    }
  }
  async select(
    member: string,
    confirm: (context: SpaceMemberContext) => Promise<boolean>,
    ticket: AdditionTicket | null,
  ) {
    const context = this.state.results.find((c) => c.member_id === member),
      owner = this.owner;
    if (
      !this.current(ticket) ||
      !ticket ||
      !owner ||
      !this.state.open ||
      !context?.can_add ||
      this.state.busy ||
      this.review ||
      this.writes.has(ticket.scope)
    )
      return;
    const review = {};
    this.review = review;
    this.publish({ busy: true, selected: context, failed: false });
    let dispatched = false;
    try {
      if (!(await confirm(context)) || !this.current(ticket) || this.review !== review) return;
      const current = validate(await this.port.read(member, owner), this.space, owner, member);
      if (!this.current(ticket) || this.review !== review) return;
      if (!current.can_add || current.revision !== context.revision) {
        await this.refresh();
        return;
      }
      this.writes.set(ticket.scope, review);
      dispatched = true;
      const result = validate(await this.port.add(current, owner), this.space, owner, member);
      if (this.current(ticket)) this.publish({ selected: result });
    } catch {
      if (this.current(ticket)) this.publish({ failed: true });
    } finally {
      if (this.review === review) this.review = null;
      if (dispatched && this.writes.get(ticket.scope) === review) this.writes.delete(ticket.scope);
      const next = this.ticket();
      if (this.current(next) && next?.scope === ticket.scope)
        this.publish({ busy: Boolean(this.review || this.writes.has(ticket.scope)) });
    }
  }
}
