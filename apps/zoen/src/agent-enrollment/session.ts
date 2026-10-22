import type { AccountDto } from '../core/generated/AccountDto.ts';
import type { AgentStatus } from '../core/generated/AgentStatus.ts';

export type EnrollmentScope = Readonly<{ owner: string; device: string; relay: string }>;
/** Display projection of a core-validated offer; this is not a wire or permission store. */
export type EnrollmentReview = Readonly<{
  agent: string;
  digest: string;
  name: string;
  handle: string;
  persona: string;
  rules: readonly string[];
  pairing: string;
  expires: bigint;
}>;
export type EnrollmentReceipt = Readonly<{
  review: EnrollmentReview;
  receipt: string;
  outcome: 'Sent' | 'Unknown';
}>;
export type EnrollmentPort = {
  preview(code: string, scope: EnrollmentScope): Promise<EnrollmentReview>;
  approve(
    code: string,
    review: EnrollmentReview,
    scope: EnrollmentScope,
  ): Promise<{ receipt: string; agent: string; digest: string; outcome: 'Sent' | 'Unknown' }>;
  receipt(scope: EnrollmentScope): Promise<EnrollmentReceipt | null>;
  dismiss(receipt: string, review: EnrollmentReview, scope: EnrollmentScope): Promise<boolean>;
  registration(review: EnrollmentReview, scope: EnrollmentScope): Promise<AgentStatus | null>;
};
export type EnrollmentState = Readonly<{
  scope: string;
  phase: 'entry' | 'checking' | 'review' | 'submitting' | 'waiting' | 'registered' | 'revoked';
  code: string;
  review: EnrollmentReview | null;
  outcome: 'Sent' | 'Unknown' | null;
  receipt: string | null;
  receiptKnown: boolean;
  error: 'preview' | 'expired' | 'status' | 'dismiss' | null;
  reconciling: boolean;
  dismissing: boolean;
}>;
export type EnrollmentInputTicket = Readonly<{ generation: number; scope: string }>;
export type EnrollmentConfirmation = Readonly<{
  ticket: EnrollmentInputTicket;
  digest: string;
  pairing: string;
}>;
export type EnrollmentRestart = Readonly<{
  ticket: EnrollmentInputTicket;
  receipt: string | null;
  digest: string;
}>;

export function enrollmentScope(mode: string, account: AccountDto | null | undefined) {
  return mode === 'account' && account?.unlocked && account.registered && !account.setup_pending
    ? { owner: account.identity_id, device: account.device_id, relay: account.relay_url }
    : null;
}
export const enrollmentScopeKey = (scope: EnrollmentScope | null) =>
  scope ? JSON.stringify([scope.owner, scope.device, scope.relay]) : '';
const entry = (scope = ''): EnrollmentState => ({
  scope,
  phase: 'entry',
  code: '',
  review: null,
  outcome: null,
  receipt: null,
  receiptKnown: false,
  error: null,
  reconciling: false,
  dismissing: false,
});
export const emptyEnrollmentState = entry();
function retainReview(review: EnrollmentReview): EnrollmentReview {
  return Object.freeze({ ...review, rules: Object.freeze([...review.rules]) });
}

/** Owns only one visible flow. Cryptography, expiry and account authority stay in core. */
export class EnrollmentSession {
  private scope: EnrollmentScope | null = null;
  private generation = 0;
  private approvalStarted = false;
  private state = entry();
  private listeners = new Set<() => void>();
  private readonly port: EnrollmentPort;
  private readonly now: () => bigint;

  constructor(port: EnrollmentPort, now: () => bigint = () => BigInt(Date.now())) {
    this.port = port;
    this.now = now;
  }

  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(state: EnrollmentState) {
    this.state = state;
    for (const listener of this.listeners) listener();
  }
  activate(scope: EnrollmentScope | null) {
    this.generation++;
    this.scope = scope ? Object.freeze({ ...scope }) : null;
    this.approvalStarted = false;
    this.publish(entry(enrollmentScopeKey(scope)));
  }
  retire() {
    this.generation++;
    this.scope = null;
    this.approvalStarted = false;
    this.publish(entry());
  }
  inputTicket(): EnrollmentInputTicket | null {
    return this.scope && !this.approvalStarted && this.state.phase !== 'checking'
      ? { generation: this.generation, scope: this.state.scope }
      : null;
  }
  accepts(ticket: EnrollmentInputTicket) {
    return (
      this.scope !== null &&
      ticket.generation === this.generation &&
      ticket.scope === this.state.scope
    );
  }
  setCode(code: string, ticket?: EnrollmentInputTicket) {
    if (!this.scope || this.approvalStarted || (ticket && !this.accepts(ticket))) return false;
    this.generation++;
    this.publish({ ...entry(this.state.scope), code });
    return true;
  }
  async check() {
    if (!this.scope || this.approvalStarted || this.state.phase !== 'entry') return;
    const code = this.state.code.trim();
    if (!code) return;
    const scope = this.scope;
    const ticket = { generation: this.generation, scope: this.state.scope };
    this.publish({ ...this.state, phase: 'checking', error: null });
    try {
      const review = retainReview(await this.port.preview(code, scope));
      if (!this.accepts(ticket)) return;
      this.publish({
        ...this.state,
        phase: 'review',
        review,
        error: review.expires <= this.now() ? 'expired' : null,
      });
    } catch {
      if (this.accepts(ticket)) this.publish({ ...this.state, phase: 'entry', error: 'preview' });
    }
  }
  expire() {
    if (this.state.phase === 'review' && this.state.review!.expires <= this.now())
      this.publish({ ...this.state, error: 'expired' });
  }
  confirmation(): EnrollmentConfirmation | null {
    const ticket = this.inputTicket();
    const review = this.state.review;
    return ticket && review && this.state.phase === 'review'
      ? { ticket, digest: review.digest, pairing: review.pairing }
      : null;
  }
  async approve(confirmation: EnrollmentConfirmation) {
    const review = this.state.review;
    if (
      !this.scope ||
      this.approvalStarted ||
      this.state.phase !== 'review' ||
      !this.accepts(confirmation.ticket) ||
      !review ||
      confirmation.digest !== review.digest ||
      confirmation.pairing !== review.pairing
    )
      return;
    if (review.expires <= this.now()) {
      this.expire();
      return;
    }
    const scope = this.scope;
    const ticket = { generation: this.generation, scope: this.state.scope };
    const code = this.state.code.trim();
    // Consume the UI action before invoking native code. An error may have arrived
    // after durable delivery, so this flow must never turn back into an Approve button.
    this.approvalStarted = true;
    this.publish({ ...this.state, code: '', phase: 'submitting', error: null });
    let outcome: 'Sent' | 'Unknown' = 'Unknown';
    let receipt: string | null = null;
    try {
      const result = await this.port.approve(code, review, scope);
      if (result.receipt && result.agent === review.agent && result.digest === review.digest) {
        outcome = result.outcome;
        receipt = result.receipt;
      }
    } catch {
      // No inference about delivery from a transport/session exception.
    }
    if (this.accepts(ticket))
      this.publish({
        ...this.state,
        phase: 'waiting',
        outcome,
        receipt,
        receiptKnown: receipt !== null,
      });
  }
  restore(receipt: EnrollmentReceipt, ticket: EnrollmentInputTicket) {
    if (!this.accepts(ticket) || this.state.phase !== 'entry' || this.state.code) return false;
    this.generation++;
    this.approvalStarted = true;
    this.publish({
      ...entry(this.state.scope),
      phase: 'waiting',
      review: retainReview(receipt.review),
      outcome: receipt.outcome,
      receipt: receipt.receipt,
      receiptKnown: true,
    });
    return true;
  }
  async reconcile() {
    if (
      !this.scope ||
      this.state.phase !== 'waiting' ||
      this.state.reconciling ||
      this.state.dismissing
    )
      return;
    const scope = this.scope;
    const review = this.state.review!;
    const ticket = { generation: this.generation, scope: this.state.scope };
    this.publish({ ...this.state, reconciling: true, error: null });
    try {
      const receipt = await this.port.receipt(scope);
      if (!this.accepts(ticket)) return;
      if (
        receipt &&
        (receipt.review.agent !== review.agent || receipt.review.digest !== review.digest)
      )
        throw new Error('Another connection is being reviewed.');
      this.publish({
        ...this.state,
        receiptKnown: true,
        receipt: receipt?.receipt ?? null,
        outcome: receipt?.outcome ?? this.state.outcome,
      });
      const registration = await this.port.registration(review, scope);
      if (!this.accepts(ticket)) return;
      this.publish({
        ...this.state,
        phase: registration === 'Revoked' ? 'revoked' : registration ? 'registered' : 'waiting',
        reconciling: false,
      });
    } catch {
      if (this.accepts(ticket))
        this.publish({ ...this.state, reconciling: false, error: 'status' });
    }
  }
  restartConfirmation(): EnrollmentRestart | null {
    return this.scope &&
      this.state.receiptKnown &&
      this.state.review &&
      !this.state.reconciling &&
      !this.state.dismissing &&
      ['waiting', 'registered', 'revoked'].includes(this.state.phase)
      ? {
          ticket: { generation: this.generation, scope: this.state.scope },
          receipt: this.state.receipt,
          digest: this.state.review.digest,
        }
      : null;
  }
  async startAnother(confirmation: EnrollmentRestart) {
    if (
      !this.scope ||
      !this.state.receiptKnown ||
      this.state.reconciling ||
      this.state.dismissing ||
      !this.accepts(confirmation.ticket) ||
      confirmation.receipt !== this.state.receipt ||
      confirmation.digest !== this.state.review?.digest ||
      !['waiting', 'registered', 'revoked'].includes(this.state.phase)
    )
      return;
    const scope = this.scope;
    const review = this.state.review!;
    const ticket = { generation: this.generation, scope: this.state.scope };
    const receipt = this.state.receipt;
    this.publish({ ...this.state, dismissing: true, error: null });
    try {
      if (receipt && !(await this.port.dismiss(receipt, review, scope))) {
        if (await this.port.receipt(scope)) throw new Error('The connection has changed.');
      }
      if (this.accepts(ticket)) this.activate(scope);
    } catch {
      if (this.accepts(ticket))
        this.publish({ ...this.state, dismissing: false, error: 'dismiss' });
    }
  }
}
