import { agentControlRevision, agentControlRevisionMax } from './agent-control-revision.ts';
import type { AccountScope } from '../core/account-scope.ts';
import { accountScopeKey } from '../core/account-scope.ts';
import type { AgentStatus } from '../core/generated/AgentStatus.ts';
import type { AgentControlDto } from '../core/generated/AgentControlDto.ts';
import type { AgentControl } from '../core/generated/AgentControl.ts';

export type AgentLifecycleAction = 'Pause' | 'Resume';
export type AgentControlPort = {
  scope(): AccountScope | null;
  read(agent: string, scope: AccountScope): Promise<AgentControlDto>;
  retry(
    agent: string,
    action: AgentControl,
    scope: AccountScope,
    revision: string,
    operationId: string,
  ): Promise<void>;
  control(
    agent: string,
    action: AgentControl,
    scope: AccountScope,
    revision: string,
  ): Promise<void>;
};
export type AgentControlState = Readonly<{
  scopeKey: string | null;
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  status: AgentStatus | null;
  busy: boolean;
  pending: AgentLifecycleAction | 'Revoke' | null;
}>;
export type AgentControlTicket = Readonly<{
  scope: string;
  agent: string;
  generation: number;
  observation: number;
  revision: string;
  action: AgentLifecycleAction;
}>;
export type AgentControlRetryTicket = Omit<AgentControlTicket, 'action'> &
  Readonly<{ action: AgentControl; operationId: string }>;
export const emptyAgentControl: AgentControlState = {
  scopeKey: null,
  phase: 'inactive',
  status: null,
  busy: false,
  pending: null,
};

/** Focus-local presentation. The relay owns lifecycle state; this is no run/queue store. */
export class AgentControlSession {
  private readonly port: AgentControlPort;
  private readonly agent: string;
  private scope: AccountScope | null = null;
  private generation = 0;
  private observation = 0;
  private revision: string | null = null;
  private operationId: string | null = null;
  private readGeneration = 0;
  private readFlight: { generation: number; again: boolean; promise: Promise<void> } | null = null;
  private active = false;
  private state: AgentControlState = emptyAgentControl;
  private readonly listeners = new Set<() => void>();

  constructor(port: AgentControlPort, agent: string) {
    this.port = port;
    this.agent = agent;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<AgentControlState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  private current(generation = this.generation): boolean {
    return (
      this.active &&
      !!this.scope &&
      generation === this.generation &&
      accountScopeKey(this.scope) === accountScopeKey(this.port.scope())
    );
  }
  activate(scope: AccountScope | null) {
    const pending =
      scope && accountScopeKey(scope) === accountScopeKey(this.scope) ? this.state.pending : null;
    ++this.generation;
    ++this.readGeneration;
    this.readFlight = null;
    this.operationId = null;
    this.revision = null;
    this.scope = scope ? Object.freeze({ ...scope }) : null;
    this.active = scope !== null;
    this.state = { ...emptyAgentControl, scopeKey: scope ? accountScopeKey(scope) : null, pending };
    this.publish({});
  }
  /** Preserve an unresolved action through background/focus changes on this route. */
  suspend() {
    ++this.generation;
    ++this.readGeneration;
    this.readFlight = null;
    this.active = false;
    this.publish({ phase: 'inactive', status: null, busy: false });
  }
  retire() {
    this.activate(null);
  }
  async refresh(observed?: AgentControlState) {
    if (this.state.busy || (observed && observed !== this.state)) return;
    await this.readStatus();
  }
  private readStatus(): Promise<void> {
    if (!this.current() || !this.scope) return Promise.resolve();
    if (this.readFlight?.generation === this.generation) {
      this.readFlight.again = true;
      if (this.state.phase !== 'loading') this.publish({ phase: 'loading' });
      return this.readFlight.promise;
    }
    const flight = { generation: this.generation, again: false, promise: Promise.resolve() };
    this.readFlight = flight;
    // Install the shared flight before publishing loading or calling the port.
    flight.promise = Promise.resolve().then(async () => {
      try {
        do {
          flight.again = false;
          if (!this.current(flight.generation) || !this.scope) return;
          const scope = this.scope;
          const read = ++this.readGeneration;
          if (this.state.phase !== 'loading') this.publish({ phase: 'loading' });
          try {
            const result = await this.port.read(this.agent, scope);
            if (!this.current(flight.generation) || read !== this.readGeneration) return;
            if (flight.again) continue;
            ++this.observation;
            // A canonical read reconciles the durable intent; it never resends.
            this.revision = result.revision;
            this.operationId = result.operation_id;
            this.publish({ phase: 'ready', status: result.status, pending: result.pending });
          } catch {
            if (!this.current(flight.generation) || read !== this.readGeneration) return;
            if (!flight.again) this.publish({ phase: 'failed', status: null });
          }
        } while (flight.again && this.current(flight.generation));
      } finally {
        if (this.readFlight === flight) this.readFlight = null;
      }
    });
    this.publish({ phase: 'loading' });
    return flight.promise;
  }
  ticket(): AgentControlTicket | null {
    const status = this.state.status;
    const revision = agentControlRevision(this.revision);
    return revision !== null &&
      revision < agentControlRevisionMax &&
      this.current() &&
      this.state.phase === 'ready' &&
      !this.state.busy &&
      !this.state.pending &&
      !!this.revision &&
      (status === 'Active' || status === 'Paused')
      ? Object.freeze({
          scope: accountScopeKey(this.scope),
          agent: this.agent,
          generation: this.generation,
          observation: this.observation,
          revision: this.revision,
          action: status === 'Active' ? 'Pause' : 'Resume',
        })
      : null;
  }
  retryTicket(): AgentControlRetryTicket | null {
    return this.current() &&
      this.state.phase === 'ready' &&
      !this.state.busy &&
      this.state.pending &&
      this.revision &&
      this.operationId
      ? Object.freeze({
          scope: accountScopeKey(this.scope),
          agent: this.agent,
          generation: this.generation,
          observation: this.observation,
          revision: this.revision,
          operationId: this.operationId,
          action: this.state.pending,
        })
      : null;
  }
  async retry(ticket: AgentControlRetryTicket) {
    const current = this.retryTicket();
    if (
      !current ||
      !this.scope ||
      ticket.scope !== current.scope ||
      ticket.agent !== current.agent ||
      ticket.generation !== current.generation ||
      ticket.observation !== current.observation ||
      ticket.revision !== current.revision ||
      ticket.operationId !== current.operationId ||
      ticket.action !== current.action
    )
      return;
    const scope = this.scope;
    this.publish({ busy: true });
    try {
      await this.port.retry(this.agent, ticket.action, scope, ticket.revision, ticket.operationId);
    } catch {
      /* The same durable intent remains unresolved; reads never resend it. */
    }
    if (!this.current(ticket.generation)) return;
    await this.readStatus();
    if (this.current(ticket.generation)) this.publish({ busy: false });
  }
  /** An admitted result belongs to its original owner/activation even after canonical refresh. */
  continues(ticket: AgentControlTicket): boolean {
    return (
      this.current(ticket.generation) &&
      ticket.scope === accountScopeKey(this.scope) &&
      ticket.agent === this.agent
    );
  }
  /** A presentation action must retain this exact scope, activation and canonical observation. */
  accepts(ticket: AgentControlTicket) {
    const current = this.ticket();
    return (
      current &&
      ticket.scope === current.scope &&
      ticket.agent === current.agent &&
      ticket.generation === current.generation &&
      ticket.observation === current.observation &&
      ticket.revision === current.revision &&
      ticket.action === current.action
    );
  }
  async change(ticket: AgentControlTicket) {
    await this.dispatch(ticket, ticket.action);
  }
  async revoke(ticket: AgentControlTicket) {
    await this.dispatch(ticket, 'Revoke');
  }
  private async dispatch(ticket: AgentControlTicket, action: AgentControl) {
    if (!this.accepts(ticket) || !this.scope) return;
    const scope = this.scope;
    // Consume before dispatch; double taps and queued stale handlers cannot send twice.
    this.publish({ busy: true, pending: action });
    try {
      await this.port.control(this.agent, action, scope, ticket.revision);
    } catch {
      // Delivery is unknown. Reconciliation is a status read, never an automatic retry.
    }
    if (!this.current(ticket.generation)) return;
    await this.readStatus();
    if (this.current(ticket.generation)) this.publish({ busy: false });
  }
}
