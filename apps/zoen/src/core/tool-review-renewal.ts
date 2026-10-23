import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { ClientStore } from './store';
import { toolApprovalExpired, toolApprovalValid } from './tool-request-decision.ts';

export type ReviewScope = Readonly<{
  identity: string;
  device: string;
  relay: string;
  epoch: number;
}>;
export type ReviewTicket = Readonly<{ scope: string; revision: number; id: string; hash: string }>;
export interface ReviewPort {
  scope(): ReviewScope | null;
  requests(): Promise<AgentRequestDto[]>;
  requestReview(id: string, hash: string): Promise<unknown>;
}
export type ReviewState = Readonly<{
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  request: AgentRequestDto | null;
  busy: boolean;
  unknown: boolean;
}>;
export const emptyReview: ReviewState = {
  phase: 'inactive',
  request: null,
  busy: false,
  unknown: false,
};
export const reviewScopeKey = (scope: ReviewScope | null) =>
  scope ? JSON.stringify([scope.identity, scope.device, scope.relay, scope.epoch]) : '';

export function createReviewPort(store: ClientStore): ReviewPort {
  return {
    scope() {
      const state = store.getSnapshot();
      const account = state.snapshot?.account;
      return state.phase === 'ready' &&
        state.mode === 'account' &&
        !state.erasePending &&
        !store.isSessionRetiring &&
        account?.registered &&
        account.unlocked &&
        !account.setup_pending
        ? {
            identity: account.identity_id,
            device: account.device_id,
            relay: account.relay_url,
            epoch: store.sessionEpoch,
          }
        : null;
    },
    requests: () => store.driver.request({ op: 'requests' }),
    requestReview: (request_id, observed_content_hash) =>
      store.request({
        op: 'decide_tool_request',
        request_id,
        observed_content_hash,
        decision: 'RequestReview',
      }),
  };
}

export function reviewMetadataValid(request: AgentRequestDto | null): boolean {
  const review = request?.tool_approval?.review;
  const id = (value: unknown): value is string =>
    typeof value === 'string' && value.length > 0 && value.length <= 128;
  return Boolean(
    request &&
    id(request.id) &&
    review &&
    id(review.root_request_id) &&
    (review.predecessor_request_id === null ||
      (id(review.predecessor_request_id) && review.predecessor_request_id !== request.id)) &&
    ['Unavailable', 'Available', 'Requested', 'Superseded', 'Reconcile'].includes(review.state) &&
    (review.state === 'Superseded'
      ? id(review.successor_request_id) && review.successor_request_id !== request.id
      : review.successor_request_id === null),
  );
}

/** UI eligibility only. The atomic Core operation verifies the signed predecessor again. */
export function reviewAvailable(request: AgentRequestDto | null, now = Date.now()): boolean {
  const tool = request?.tool_approval;
  return Boolean(
    request?.agent.is_mine &&
    request.status === 'Approved' &&
    tool &&
    toolApprovalValid(tool) &&
    toolApprovalExpired(tool, now) &&
    reviewMetadataValid(request) &&
    tool.review?.state === 'Available',
  );
}

/** Focus-local presentation; no approval, retry queue or durable second store. */
export class ToolReviewSession {
  private state: ReviewState = emptyReview;
  private scope: ReviewScope | null = null;
  private revision = 0;
  private flight: ReviewTicket | null = null;
  private readonly listeners = new Set<() => void>();
  private readonly port: ReviewPort;
  private readonly id: string;
  constructor(port: ReviewPort, id: string) {
    this.port = port;
    this.id = id;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<ReviewState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  activate(scope: ReviewScope | null) {
    this.scope = scope ? Object.freeze({ ...scope }) : null;
    ++this.revision;
    this.flight = null;
    this.state = { ...emptyReview };
    this.publish({});
  }
  retire() {
    this.activate(null);
  }
  private current(revision = this.revision): boolean {
    return Boolean(
      this.scope &&
      revision === this.revision &&
      reviewScopeKey(this.scope) === reviewScopeKey(this.port.scope()),
    );
  }
  async refresh() {
    if (!this.current()) return;
    const revision = ++this.revision;
    this.publish({ phase: 'loading', busy: Boolean(this.flight) });
    try {
      const requests = await this.port.requests();
      if (!this.current(revision)) return;
      const matching = requests.filter((request) => request.id === this.id);
      const request = matching.length === 1 ? matching[0]! : null;
      const observedIntent = ['Requested', 'Superseded', 'Reconcile'].includes(
        request?.tool_approval?.review?.state ?? '',
      );
      this.publish({ phase: 'ready', request, unknown: this.state.unknown && !observedIntent });
    } catch {
      if (this.current(revision)) this.publish({ phase: 'failed', request: null });
    }
  }
  ticket(): ReviewTicket | null {
    const request = this.state.request;
    return this.current() &&
      this.state.phase === 'ready' &&
      !this.flight &&
      !this.state.busy &&
      !this.state.unknown &&
      reviewAvailable(request)
      ? Object.freeze({
          scope: reviewScopeKey(this.scope),
          revision: this.revision,
          id: this.id,
          hash: request!.tool_approval!.content_hash,
        })
      : null;
  }
  successor(): string | null {
    const request = this.state.request;
    return this.current() &&
      this.state.phase === 'ready' &&
      !this.flight &&
      !this.state.unknown &&
      request?.agent.is_mine &&
      request.status === 'Approved' &&
      request.tool_approval &&
      toolApprovalValid(request.tool_approval) &&
      reviewMetadataValid(request) &&
      request.tool_approval.review?.state === 'Superseded'
      ? request.tool_approval.review.successor_request_id
      : null;
  }
  private accepts(ticket: ReviewTicket | null): ticket is ReviewTicket {
    return Boolean(
      ticket &&
      this.current(ticket.revision) &&
      ticket.scope === reviewScopeKey(this.scope) &&
      ticket.id === this.id &&
      ticket.hash === this.state.request?.tool_approval?.content_hash &&
      this.state.phase === 'ready' &&
      !this.state.unknown &&
      reviewAvailable(this.state.request),
    );
  }
  async renew(ticket: ReviewTicket | null) {
    if (!this.accepts(ticket) || this.flight || this.state.busy) return false;
    this.flight = ticket;
    let dispatched = false;
    this.publish({ busy: true });
    try {
      // The button requests preparation only. Read current Core state and
      // compare the exact predecessor before dispatching that inert owner intent.
      const requests = await this.port.requests();
      if (!this.accepts(ticket)) return false;
      const matching = requests.filter((request) => request.id === ticket.id);
      const request = matching.length === 1 ? matching[0]! : null;
      if (
        !reviewAvailable(request) ||
        request!.agent.id !== this.state.request!.agent.id ||
        request!.tool_approval!.content_hash !== ticket.hash
      ) {
        this.publish({ request });
        return false;
      }
      dispatched = true;
      await this.port.requestReview(ticket.id, ticket.hash);
      if (this.current(ticket.revision)) await this.refresh();
      return (
        this.current() && ticket.scope === reviewScopeKey(this.scope) && this.flight === ticket
      );
    } catch {
      if (this.flight === ticket && this.current() && ticket.scope === reviewScopeKey(this.scope)) {
        if (dispatched) {
          this.publish({ unknown: true });
          // Read-only reconciliation; neither an error nor a refresh resends it.
          await this.refresh();
        } else this.publish({ phase: 'failed', request: null });
      }
      return false;
    } finally {
      if (this.flight === ticket) {
        this.flight = null;
        if (this.current()) this.publish({ busy: false });
      }
    }
  }
}
