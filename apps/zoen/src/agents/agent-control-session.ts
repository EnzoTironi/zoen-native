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
    action: AgentLifecycleAction,
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
      accountScopeKey(scope) === accountScopeKey(this.scope) ? this.state.pending : null;
    ++this.generation;
    ++this.readGeneration;
    this.operationId = null;
    this.scope = scope ? Object.freeze({ ...scope }) : null;
    this.active = scope !== null;
    this.state = { ...emptyAgentControl, scopeKey: accountScopeKey(scope), pending };
    this.publish({});
  }
  /** Preserve an unresolved action through background/focus changes on this route. */
  suspend() {
    ++this.generation;
    ++this.readGeneration;
    this.active = false;
    this.publish({ phase: 'inactive', status: null, busy: false });
  }
  retire() {
    this.scope = null;
    this.suspend();
    this.publish({ pending: null });
  }
  async refresh() {
    if (!this.current() || !this.scope) return;
    const scope = this.scope;
    const generation = this.generation;
    const read = ++this.readGeneration;
    this.publish({ phase: 'loading' });
    try {
      const result = await this.port.read(this.agent, scope);
      if (!this.current(generation) || read !== this.readGeneration) return;
      ++this.observation;
      // Core reopens the durable intent and reconciles the exact relay revision.
      // Matching status alone cannot prove which request won. Reads never resend.
      this.revision = result.revision;
      this.operationId = result.operation_id;
      this.publish({
        phase: 'ready',
        status: result.status,
        pending: result.pending,
      });
    } catch {
      if (this.current(generation) && read === this.readGeneration)
        this.publish({ phase: 'failed', status: null });
    }
  }
  ticket(): AgentControlTicket | null {
    const status = this.state.status;
    return this.current() &&
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
    await this.refresh();
    if (this.current(ticket.generation)) this.publish({ busy: false });
  }
  private accepts(ticket: AgentControlTicket) {
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
    if (!this.accepts(ticket) || !this.scope) return;
    const scope = this.scope;
    // Consume before dispatch; double taps and queued stale handlers cannot send twice.
    this.publish({ busy: true, pending: ticket.action });
    try {
      await this.port.control(this.agent, ticket.action, scope, ticket.revision);
    } catch {
      // Delivery is unknown. Reconciliation is a status read, never an automatic retry.
    }
    if (!this.current(ticket.generation)) return;
    await this.refresh();
    if (this.current(ticket.generation)) this.publish({ busy: false });
  }
}
