import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { ManagedEnrollmentDto } from '../core/generated/ManagedEnrollmentDto.ts';

export type ManagedAgentPort = {
  status(scope: AccountScope): Promise<ManagedEnrollmentDto | null>;
  start(name: string, scope: AccountScope): Promise<ManagedEnrollmentDto>;
  approve(view: ManagedEnrollmentDto, scope: AccountScope): Promise<ManagedEnrollmentDto>;
  verify(view: ManagedEnrollmentDto, scope: AccountScope): Promise<void>;
  chat(view: ManagedEnrollmentDto, scope: AccountScope): Promise<string>;
  discard(view: ManagedEnrollmentDto, scope: AccountScope): Promise<void>;
  complete(view: ManagedEnrollmentDto, scope: AccountScope): Promise<void>;
};
export type ManagedAgentState = Readonly<{
  ready: boolean;
  view: ManagedEnrollmentDto | null;
  busy: boolean;
  error: 'status' | 'start' | 'approve' | 'open' | null;
}>;
const empty = (): ManagedAgentState => ({ ready: false, view: null, busy: false, error: null });

/** UI projection only. Native Core retains the operation, offer and approval. */
export class ManagedAgentSession {
  private owner: AccountScope | null = null;
  private generation = 0;
  private state = empty();
  private listeners = new Set<() => void>();
  private port: ManagedAgentPort;
  constructor(port: ManagedAgentPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  ticket() {
    return this.owner ? this.generation : null;
  }
  accepts(ticket: number | null) {
    return ticket !== null && !!this.owner && ticket === this.generation;
  }
  private publish(update: Partial<ManagedAgentState>) {
    this.state = { ...this.state, ...update };
    this.listeners.forEach((listener) => listener());
  }
  activate(owner: AccountScope | null) {
    this.generation++;
    this.owner = owner;
    this.state = empty();
    this.publish({});
  }
  retire() {
    this.activate(null);
  }
  scopeMatches(owner: AccountScope | null) {
    return !!owner && accountScopeKey(owner) === accountScopeKey(this.owner);
  }
  async refresh() {
    if (!this.owner || this.state.busy) return;
    const ticket = this.ticket(),
      owner = this.owner;
    this.publish({ busy: true, error: null });
    try {
      const view = await this.port.status(owner);
      if (this.accepts(ticket)) this.publish({ ready: true, view });
    } catch {
      if (this.accepts(ticket)) this.publish({ error: 'status' });
    } finally {
      if (this.accepts(ticket)) this.publish({ busy: false });
    }
  }
  async start(name: string, observedTicket = this.ticket()) {
    if (!this.accepts(observedTicket)) return;
    if (!this.owner || !this.state.ready || this.state.view || this.state.busy || !name.trim())
      return;
    const ticket = this.ticket(),
      owner = this.owner;
    this.publish({ busy: true, error: null });
    try {
      const view = await this.port.start(name.trim(), owner);
      if (this.accepts(ticket)) this.publish({ view });
    } catch {
      // A lost create ACK may still have retained an intent. Require a native
      // status read before displaying an empty form or accepting another name.
      if (this.accepts(ticket)) this.publish({ ready: false, error: 'start' });
    } finally {
      if (this.accepts(ticket)) this.publish({ busy: false });
    }
  }
  async approve(observedTicket = this.ticket(), observed = this.state.view) {
    if (
      !this.accepts(observedTicket) ||
      observed?.operation_id !== this.state.view?.operation_id ||
      observed?.preview?.offer_digest !== this.state.view?.preview?.offer_digest
    )
      return;
    const view = this.state.view;
    if (
      !this.owner ||
      this.state.busy ||
      this.state.error ||
      view?.state !== 'review' ||
      !view.preview
    )
      return;
    const ticket = this.ticket(),
      owner = this.owner;
    this.publish({ busy: true, error: null });
    try {
      const next = await this.port.approve(view, owner);
      if (this.accepts(ticket)) this.publish({ view: next });
    } catch {
      // Never automatically resubmit an ambiguous approval. Read its receipt.
      if (this.accepts(ticket)) this.publish({ error: 'approve' });
    } finally {
      if (this.accepts(ticket)) this.publish({ busy: false });
    }
  }
  async restart(observedTicket = this.ticket(), operation = this.state.view?.operation_id) {
    if (!this.accepts(observedTicket) || operation !== this.state.view?.operation_id) return;
    const view = this.state.view;
    if (!this.owner || this.state.busy || !view || !['expired', 'rejected'].includes(view.state))
      return;
    const ticket = this.ticket(),
      owner = this.owner;
    this.publish({ busy: true, error: null });
    try {
      await this.port.discard(view, owner);
      if (this.accepts(ticket)) this.publish({ view: null, ready: true });
    } catch {
      // An ACK loss requires status recovery before enabling another intent.
      if (this.accepts(ticket)) this.publish({ error: 'status', ready: false });
    } finally {
      if (this.accepts(ticket)) this.publish({ busy: false });
    }
  }
  async open(
    destination: 'budget' | 'chat',
    observedTicket = this.ticket(),
    operation = this.state.view?.operation_id,
  ): Promise<string | null> {
    if (!this.accepts(observedTicket) || operation !== this.state.view?.operation_id) return null;
    const view = this.state.view;
    if (!this.owner || this.state.busy || view?.state !== 'registered' || !view.agent_id)
      return null;
    const ticket = this.ticket(),
      owner = this.owner;
    this.publish({ busy: true, error: null });
    try {
      await this.port.verify(view, owner);
      if (!this.accepts(ticket)) return null;
      if (destination === 'budget') return view.agent_id;
      const space = await this.port.chat(view, owner);
      if (!this.accepts(ticket)) return null;
      await this.port.complete(view, owner);
      return this.accepts(ticket) ? space : null;
    } catch {
      if (this.accepts(ticket)) this.publish({ error: 'open' });
      return null;
    } finally {
      if (this.accepts(ticket)) this.publish({ busy: false });
    }
  }
}
