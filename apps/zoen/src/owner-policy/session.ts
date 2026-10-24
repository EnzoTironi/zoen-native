import {
  digest,
  units,
  frozenContext,
  frozenReview,
  effectiveness,
  validContext,
  validReview,
} from './values.ts';
import type { AccountScope } from '../core/account-scope.ts';
import { accountScopeKey } from '../core/account-scope.ts';
import type { OwnerPolicyContext } from '../core/generated/OwnerPolicyContext.ts';
import type { OwnerPolicyReview } from '../core/generated/OwnerPolicyReview.ts';
import type { OwnerPolicyStatus } from '../core/generated/OwnerPolicyStatus.ts';
import type { OwnerPolicyOptionsStatus } from '../core/generated/OwnerPolicyOptionsStatus.ts';

export type PolicyTarget = Readonly<
  { spaceId: string; agentId: string } & (
    | { purpose: 'account'; requestId?: never; expectedHash?: never }
    | { purpose?: 'image'; requestId: string; expectedHash: string }
  )
>;
export type PolicyScope = AccountScope & PolicyTarget;
export const policyScopeKey = (scope: PolicyScope | null) =>
  scope
    ? JSON.stringify([
        accountScopeKey(scope),
        scope.spaceId,
        scope.agentId,
        scope.purpose ?? 'image',
        scope.requestId,
        scope.expectedHash,
      ])
    : '';
export type PolicyPort = {
  scope(): PolicyScope | null;
  inquire(scope: PolicyScope): Promise<OwnerPolicyOptionsStatus>;
  options(scope: PolicyScope, source: string): Promise<OwnerPolicyOptionsStatus>;
  findOptions(scope: PolicyScope): Promise<OwnerPolicyOptionsStatus | null>;
  prepare(
    scope: PolicyScope,
    source: string,
    digest: string,
    limit: string,
    attempt: string,
    enabled: boolean,
  ): Promise<OwnerPolicyReview>;
  submit(scope: PolicyScope, review: OwnerPolicyReview): Promise<OwnerPolicyStatus>;
  status(scope: PolicyScope, source: string): Promise<OwnerPolicyStatus>;
  find(scope: PolicyScope, digest: string): Promise<OwnerPolicyStatus | null>;
  changed(scope: PolicyScope): void;
};
export type PolicyState = Readonly<{
  scope: string;
  phase:
    | 'unavailable'
    | 'loading'
    | 'editing'
    | 'preparing'
    | 'review'
    | 'submitting'
    | 'pending'
    | 'installed'
    | 'refused'
    | 'held';
  context: OwnerPolicyContext | null;
  review: OwnerPolicyReview | null;
  receipt: OwnerPolicyStatus | null;
  error: 'options' | 'prepare' | 'submit' | 'reconcile' | 'invalid-response' | null;
  limitInput: string;
  maxAttemptInput: string;
  enabled: boolean;
  optionsState: 'pending' | 'ready' | 'refused' | 'delivery_failed' | 'expired' | null;
  optionsSource: string | null;
  inquiryDigest: string | null;
  effectiveness: 'unknown' | 'current' | 'superseded' | 'expired' | 'disabled';
  refreshing: boolean;
}>;
export type PolicyConfirmation = Readonly<{
  scope: string;
  generation: number;
  revision: number;
  digest: string;
  policyDigest: string;
  optionsDigest: string;
}>;
const initial = (scope = ''): PolicyState => ({
  scope,
  phase: 'unavailable',
  context: null,
  review: null,
  receipt: null,
  error: null,
  limitInput: '',
  maxAttemptInput: '',
  enabled: true,
  optionsState: null,
  optionsSource: null,
  inquiryDigest: null,
  effectiveness: 'unknown',
  refreshing: false,
});
export const emptyPolicyState = initial();
/** Disposable review state only; Core and Runtime own all durable financial facts. */
export class OwnerPolicySession {
  private owner: PolicyScope | null = null;
  private generation = 0;
  private revision = 0;
  private readSequence = 0;
  private inquiring = false;
  private preparing = false;
  private submitting = false;
  private reading = false;
  private inquiryUncertain = false;
  private previousQuery: string | null = null;
  private dirty = false;
  private submitted: OwnerPolicyReview | null = null;
  private receiptRefresh: string | null = null;
  private state = emptyPolicyState;
  private listeners = new Set<() => void>();
  private readonly port: PolicyPort;
  constructor(port: PolicyPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<PolicyState>) {
    this.state = Object.freeze({ ...this.state, ...patch });
    for (const listener of this.listeners) listener();
  }
  private current(generation = this.generation) {
    return (
      !!this.owner &&
      generation === this.generation &&
      policyScopeKey(this.owner) === policyScopeKey(this.port.scope())
    );
  }
  activate(scope: PolicyScope | null) {
    const key = policyScopeKey(scope),
      resume = key !== '' && this.state.scope === key;
    this.generation++;
    this.revision++;
    this.readSequence++;
    this.owner = scope ? Object.freeze({ ...scope }) : null;
    this.inquiring = false;
    this.preparing = false;
    this.submitting = false;
    this.reading = false;
    if (!resume) {
      this.dirty = false;
      this.inquiryUncertain = false;
      this.previousQuery = null;
      this.submitted = null;
      this.receiptRefresh = null;
      this.state = initial(key);
    }
    this.publish({
      scope: key,
      phase: scope ? (this.submitted ? 'held' : 'loading') : 'unavailable',
      context: null,
      review: this.submitted,
      optionsState: resume ? this.state.optionsState : null,
      optionsSource: resume ? this.state.optionsSource : null,
      inquiryDigest: resume ? this.state.inquiryDigest : null,
      effectiveness: 'unknown',
      refreshing: false,
      error: null,
    });
  }
  retire() {
    this.generation++;
    this.revision++;
    this.readSequence++;
    this.owner = null;
    this.publish({
      phase: 'unavailable',
      context: null,
      review: null,
      effectiveness: 'unknown',
      refreshing: false,
    });
  }
  private adoptOptions(value: OwnerPolicyOptionsStatus, expectedSource?: string) {
    if (
      !value.source ||
      !digest(value.inquiry_digest) ||
      (expectedSource && value.source !== expectedSource) ||
      (this.state.inquiryDigest &&
        expectedSource &&
        value.inquiry_digest !== this.state.inquiryDigest) ||
      !['pending', 'ready', 'refused', 'delivery_failed', 'expired'].includes(value.state) ||
      (value.state === 'ready') !== !!value.context ||
      (value.context && !validContext(value.context))
    )
      throw Error('Invalid options');
    const context = value.context ? frozenContext(value.context) : null;
    const replaced = !!context && this.state.context?.options_digest !== context.options_digest;
    const phase = this.submitted
      ? this.state.phase
      : value.state === 'ready'
        ? 'editing'
        : value.state === 'pending'
          ? 'loading'
          : 'refused';
    if (replaced) this.revision++;
    this.publish({
      optionsState:
        value.state === 'pending' ||
        value.state === 'ready' ||
        value.state === 'refused' ||
        value.state === 'expired'
          ? value.state
          : 'delivery_failed',
      optionsSource: value.source,
      inquiryDigest: value.inquiry_digest,
      context,
      phase: !this.submitted && this.state.phase === 'review' && !replaced ? 'review' : phase,
      review: replaced && !this.submitted ? null : this.state.review,
      limitInput:
        context && !this.dirty ? (context.current_limit_units ?? '') : this.state.limitInput,
      maxAttemptInput:
        context && !this.dirty
          ? (context.current_max_attempt_units ?? context.reservation_units)
          : this.state.maxAttemptInput,
      enabled: context && !this.dirty ? (context.current_enabled ?? true) : this.state.enabled,
      error: null,
      effectiveness: effectiveness(this.state.receipt, context),
    });
  }
  private adoptReceipt(value: OwnerPolicyStatus) {
    if (
      !this.submitted ||
      value.policy_digest !== this.submitted.policy_digest ||
      !value.source ||
      !['pending', 'installed', 'refused', 'delivery_failed'].includes(value.state) ||
      (this.state.receipt && this.state.receipt.source !== value.source)
    )
      throw Error('Invalid receipt');
    if (this.state.receipt?.state === 'installed' && value.state !== 'installed')
      throw Error('Installed receipt regressed');
    if (value.state !== 'pending') this.dirty = false;
    const context =
      value.state === 'installed' && this.state.receipt?.state !== 'installed'
        ? null
        : this.state.context;
    this.publish({
      context,
      receipt: Object.freeze({ ...value }),
      phase:
        value.state === 'installed'
          ? 'installed'
          : value.state === 'pending'
            ? 'pending'
            : 'refused',
      error: null,
      effectiveness: effectiveness(value, context),
    });
  }
  async refresh(observed: PolicyState = this.state) {
    if (observed !== this.state || !this.current() || this.inquiring || this.submitting) return;
    // A pending or unacknowledged inquiry already has an original. Reading it
    // cannot accidentally queue a second inquiry when the screen refocuses.
    if (this.inquiryUncertain || this.state.optionsState === 'pending') {
      await this.reconcile(observed);
      return;
    }
    const scope = this.owner!,
      generation = this.generation;
    this.inquiring = true;
    this.inquiryUncertain = true;
    this.previousQuery = this.state.optionsSource;
    this.readSequence++;
    this.revision++;
    this.publish({
      refreshing: true,
      optionsState: null,
      optionsSource: null,
      inquiryDigest: null,
      context: null,
      review: this.submitted,
      error: null,
      effectiveness: 'unknown',
      phase: this.submitted ? this.state.phase : 'loading',
    });
    try {
      const value = await this.port.inquire(scope);
      if (!this.current(generation)) return;
      this.adoptOptions(value);
      this.inquiryUncertain = false;
      this.port.changed(scope);
    } catch {
      if (this.current(generation)) this.publish({ error: 'options', phase: 'held' });
    } finally {
      if (this.current(generation)) {
        this.inquiring = false;
        this.publish({ refreshing: false });
        void this.reconcile();
      }
    }
  }
  canRetryOptions(observed: PolicyState = this.state) {
    return (
      observed === this.state &&
      this.current() &&
      !this.inquiring &&
      !this.submitting &&
      !this.inquiryUncertain &&
      !observed.refreshing &&
      !!observed.optionsSource &&
      ['refused', 'delivery_failed', 'expired'].includes(observed.optionsState ?? '')
    );
  }
  async retryOptions(observed: PolicyState = this.state) {
    if (this.canRetryOptions(observed)) await this.refresh(observed);
  }
  invalidate() {
    if (!this.current()) return;
    this.revision++;
    this.readSequence++;
    this.publish({ refreshing: true, error: null });
  }
  beginEdit(observed: PolicyState = this.state) {
    const context = this.state.context;
    if (
      observed !== this.state ||
      !this.current() ||
      !context ||
      this.state.refreshing ||
      !['installed', 'refused'].includes(this.state.phase) ||
      !['installed', 'refused'].includes(this.state.receipt?.state ?? '')
    )
      return;
    this.revision++;
    this.readSequence++;
    this.dirty = false;
    this.submitted = null;
    this.receiptRefresh = null;
    this.publish({
      phase: 'editing',
      review: null,
      receipt: null,
      error: null,
      effectiveness: 'unknown',
      limitInput: context.current_limit_units ?? '',
      maxAttemptInput: context.current_max_attempt_units ?? context.reservation_units,
      enabled: context.current_enabled ?? true,
    });
  }
  edit(
    observed: PolicyState,
    patch: { limitInput?: string; maxAttemptInput?: string; enabled?: boolean },
  ) {
    if (
      observed !== this.state ||
      !this.current() ||
      !['editing', 'preparing', 'review', 'installed', 'refused'].includes(this.state.phase) ||
      this.state.refreshing ||
      !this.state.context
    )
      return;
    if (
      this.submitted &&
      !['installed', 'refused', 'delivery_failed'].includes(this.state.receipt?.state ?? '')
    )
      return;
    this.submitted = null;
    this.receiptRefresh = null;
    this.revision++;
    this.dirty = true;
    this.publish({
      ...patch,
      phase: 'editing',
      review: null,
      receipt: null,
      effectiveness: 'unknown',
      error: null,
    });
  }
  async prepare(observed: PolicyState = this.state) {
    if (
      observed !== this.state ||
      !this.current() ||
      this.preparing ||
      this.submitted ||
      this.state.phase !== 'editing' ||
      this.state.refreshing ||
      !this.state.context ||
      !this.state.optionsSource ||
      !units(this.state.limitInput) ||
      !units(this.state.maxAttemptInput)
    )
      return;
    const scope = this.owner!,
      generation = this.generation,
      revision = this.revision,
      source = this.state.optionsSource,
      context = this.state.context;
    this.preparing = true;
    this.publish({ phase: 'preparing', error: null });
    try {
      const review = await this.port.prepare(
        scope,
        source,
        context.options_digest,
        this.state.limitInput,
        this.state.maxAttemptInput,
        this.state.enabled,
      );
      if (
        !this.current(generation) ||
        revision !== this.revision ||
        this.state.context?.options_digest !== context.options_digest
      )
        return;
      if (!validReview(review, scope, this.state)) throw Error('Invalid review');
      this.publish({ phase: 'review', review: frozenReview(review) });
    } catch {
      if (this.current(generation) && revision === this.revision)
        this.publish({ phase: 'editing', error: 'prepare' });
    } finally {
      if (this.current(generation)) this.preparing = false;
    }
  }
  confirmation(observed: PolicyState = this.state): PolicyConfirmation | null {
    return observed === this.state &&
      this.current() &&
      observed.phase === 'review' &&
      !observed.refreshing &&
      !!observed.review &&
      !!observed.context
      ? Object.freeze({
          scope: observed.scope,
          generation: this.generation,
          revision: this.revision,
          digest: observed.review.confirmation_digest,
          policyDigest: observed.review.policy_digest,
          optionsDigest: observed.context.options_digest,
        })
      : null;
  }
  async submit(ticket: PolicyConfirmation) {
    const expected = this.confirmation();
    if (
      !expected ||
      !this.owner ||
      this.submitting ||
      ticket.scope !== expected.scope ||
      ticket.generation !== expected.generation ||
      ticket.revision !== expected.revision ||
      ticket.digest !== expected.digest ||
      ticket.policyDigest !== expected.policyDigest ||
      ticket.optionsDigest !== expected.optionsDigest
    )
      return;
    const scope = this.owner,
      generation = this.generation,
      review = this.state.review!;
    this.submitted = review;
    this.submitting = true;
    this.publish({ phase: 'submitting', error: null });
    try {
      const value = await this.port.submit(scope, review);
      if (!this.current(generation)) return;
      this.adoptReceipt(value);
      this.port.changed(scope);
    } catch {
      if (this.current(generation))
        this.publish({ phase: 'held', error: 'submit', effectiveness: 'unknown' });
    } finally {
      if (this.current(generation)) {
        this.submitting = false;
        void this.reconcile();
      }
    }
  }
  async reconcile(observed: PolicyState = this.state) {
    if (
      observed !== this.state ||
      !this.current() ||
      this.reading ||
      this.inquiring ||
      this.submitting
    )
      return;
    const scope = this.owner!,
      generation = this.generation,
      sequence = ++this.readSequence,
      source = this.state.optionsSource;
    const current = () => this.current(generation) && sequence === this.readSequence;
    this.reading = true;
    try {
      if (this.inquiryUncertain) {
        const original = await this.port.findOptions(scope);
        if (!current()) return;
        if (original && original.source !== this.previousQuery) {
          this.adoptOptions(original);
          this.inquiryUncertain = false;
        } else this.publish({ phase: 'held', error: 'options', refreshing: false });
      }
      if (this.submitted) {
        const receipt = this.state.receipt
          ? await this.port.status(scope, this.state.receipt.source)
          : await this.port.find(scope, this.submitted.policy_digest);
        if (!current()) return;
        if (receipt) this.adoptReceipt(receipt);
        else this.publish({ phase: 'held', error: 'reconcile', effectiveness: 'unknown' });
      }
      if (
        (source || this.state.optionsSource) &&
        !(
          this.state.receipt?.state === 'installed' &&
          this.receiptRefresh !== this.state.receipt.policy_digest
        )
      ) {
        const originalSource = this.state.optionsSource || source;
        if (!originalSource) return;
        const options = await this.port.options(scope, originalSource);
        if (!current()) return;
        this.adoptOptions(options, originalSource);
      }
      if (current()) this.publish({ refreshing: false });
    } catch {
      if (current()) this.publish({ error: 'reconcile', refreshing: false });
    } finally {
      if (this.current(generation)) {
        this.reading = false;
        if (sequence !== this.readSequence) {
          void this.reconcile();
        } else {
          const receipt = this.state.receipt;
          if (receipt?.state === 'installed' && this.receiptRefresh !== receipt.policy_digest) {
            this.receiptRefresh = receipt.policy_digest;
            void this.refresh();
          }
        }
      }
    }
  }
}
