import type { AgentDefinitionDto } from '../core/generated/AgentDefinitionDto.ts';
import type { AgentDefinitionPreviewDto } from '../core/generated/AgentDefinitionPreviewDto.ts';
import type { AgentDefinitionUpdateDto } from '../core/generated/AgentDefinitionUpdateDto.ts';
import { enrollmentScopeKey, type EnrollmentScope } from '../agent-enrollment/session.ts';

export type DefinitionScope = EnrollmentScope & { agent: string };
export const definitionScopeKey = (scope: DefinitionScope | null) =>
  scope ? JSON.stringify([enrollmentScopeKey(scope), scope.agent]) : '';
export type DefinitionPort = {
  read(scope: DefinitionScope): Promise<AgentDefinitionDto>;
  preview(
    scope: DefinitionScope,
    baseline: AgentDefinitionDto,
    persona: string,
    rules: string[],
  ): Promise<AgentDefinitionPreviewDto>;
  update(
    scope: DefinitionScope,
    review: AgentDefinitionPreviewDto,
  ): Promise<AgentDefinitionUpdateDto>;
  reconcile(scope: DefinitionScope): Promise<AgentDefinitionUpdateDto | null>;
  dismiss(scope: DefinitionScope, receipt: AgentDefinitionUpdateDto): Promise<boolean>;
};
export type DefinitionState = Readonly<{
  scope: string;
  phase:
    | 'loading'
    | 'unavailable'
    | 'error'
    | 'editing'
    | 'previewing'
    | 'review'
    | 'submitting'
    | 'held';
  current: AgentDefinitionDto | null;
  persona: string;
  rules: readonly string[];
  ruleKeys: readonly number[];
  review: AgentDefinitionPreviewDto | null;
  receipt: AgentDefinitionUpdateDto | null;
  receiptKnown: boolean;
  error: 'load' | 'preview' | 'reconcile' | 'dismiss' | null;
  reconciling: boolean;
  dismissing: boolean;
}>;
export type DefinitionTicket = Readonly<{ generation: number; revision: number; scope: string }>;
export type DefinitionConfirmation = DefinitionTicket & { digest: string };
export type DefinitionDismissal = DefinitionConfirmation & { receipt: string | null };
const initial = (scope = ''): DefinitionState => ({
  scope,
  phase: 'loading',
  current: null,
  persona: '',
  rules: [],
  ruleKeys: [],
  review: null,
  receipt: null,
  receiptKnown: false,
  error: null,
  reconciling: false,
  dismissing: false,
});
export const emptyDefinitionState = initial();
const sameText = (
  a: Pick<AgentDefinitionDto, 'persona' | 'rules'>,
  b: Pick<AgentDefinitionDto, 'persona' | 'rules'>,
) => a.persona === b.persona && JSON.stringify(a.rules) === JSON.stringify(b.rules);
const sameDefinition = (a: AgentDefinitionDto, b: AgentDefinitionDto) =>
  a.agent_id === b.agent_id &&
  a.owner_id === b.owner_id &&
  a.device_id === b.device_id &&
  a.relay_url === b.relay_url &&
  a.status === b.status &&
  a.version === b.version &&
  a.definition_sha256 === b.definition_sha256 &&
  sameText(a, b);
function freezeDefinition(value: AgentDefinitionDto): AgentDefinitionDto {
  const copy = { ...value, rules: [...value.rules] };
  Object.freeze(copy.rules);
  return Object.freeze(copy);
}
function freezeReview(value: AgentDefinitionPreviewDto): AgentDefinitionPreviewDto {
  return Object.freeze({
    ...value,
    current: freezeDefinition(value.current),
    proposed: freezeDefinition(value.proposed),
  });
}

/** Transient human review only. Core owns versions, authorization and held attempts. */
export class DefinitionSession {
  private scope: DefinitionScope | null = null;
  private generation = 0;
  private revision = 0;
  private ruleKey = 0;
  private loading = false;
  private state = initial();
  private listeners = new Set<() => void>();
  private readonly port: DefinitionPort;
  constructor(port: DefinitionPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(state: DefinitionState) {
    this.state = state;
    for (const listener of this.listeners) listener();
  }
  activate(scope: DefinitionScope | null) {
    this.generation++;
    this.revision++;
    this.loading = false;
    this.scope = scope ? Object.freeze({ ...scope }) : null;
    this.publish({
      ...initial(definitionScopeKey(scope)),
      phase: scope ? 'loading' : 'unavailable',
    });
  }
  retire() {
    this.activate(null);
  }
  ticket(observed: DefinitionState = this.state): DefinitionTicket | null {
    return this.scope && observed === this.state
      ? { generation: this.generation, revision: this.revision, scope: this.state.scope }
      : null;
  }
  acceptsSession(ticket: DefinitionTicket) {
    return (
      !!this.scope && ticket.generation === this.generation && ticket.scope === this.state.scope
    );
  }
  accepts(ticket: DefinitionTicket) {
    return this.acceptsSession(ticket) && ticket.revision === this.revision;
  }
  private valid(value: AgentDefinitionDto) {
    const scope = this.scope;
    return (
      !!scope &&
      value.agent_id === scope.agent &&
      value.owner_id === scope.owner &&
      value.device_id === scope.device &&
      value.relay_url === scope.relay &&
      Number.isSafeInteger(value.version) &&
      value.version > 0 &&
      value.version <= 2147483647 &&
      !!value.definition_sha256 &&
      ['Active', 'Paused', 'Revoked'].includes(value.status) &&
      typeof value.persona === 'string' &&
      Array.isArray(value.rules) &&
      value.rules.every((rule) => typeof rule === 'string')
    );
  }
  private validReview(value: AgentDefinitionPreviewDto) {
    return (
      !!value.preview_digest &&
      this.valid(value.current) &&
      this.valid(value.proposed) &&
      value.proposed.version === value.current.version + 1
    );
  }
  private adopt(receipt: AgentDefinitionUpdateDto, expected?: AgentDefinitionPreviewDto) {
    if (
      !receipt.receipt_id ||
      !this.validReview(receipt.review) ||
      (expected &&
        (receipt.review.preview_digest !== expected.preview_digest ||
          !sameDefinition(receipt.review.current, expected.current) ||
          !sameDefinition(receipt.review.proposed, expected.proposed))) ||
      (receipt.current && !this.valid(receipt.current))
    )
      throw Error('Invalid held update');
    if (!['Confirmed', 'Conflict', 'Unknown'].includes(receipt.outcome))
      throw Error('Invalid outcome');
    if (
      receipt.outcome === 'Confirmed' &&
      (!receipt.current ||
        receipt.current.status === 'Revoked' ||
        receipt.current.version !== receipt.review.proposed.version ||
        receipt.current.definition_sha256 !== receipt.review.proposed.definition_sha256 ||
        !sameText(receipt.current, receipt.review.proposed))
    )
      throw Error('Unconfirmed definition');
    const review = freezeReview(receipt.review);
    const current = receipt.current ? freezeDefinition(receipt.current) : null;
    this.publish({
      ...this.state,
      phase: 'held',
      current,
      review,
      persona: review.proposed.persona,
      rules: review.proposed.rules,
      receipt: Object.freeze({ ...receipt, review, current }),
      receiptKnown: true,
      error: null,
      reconciling: false,
    });
  }
  async load() {
    if (!this.scope || this.loading || !['loading', 'error'].includes(this.state.phase)) return;
    this.loading = true;
    const scope = this.scope,
      ticket = this.ticket()!;
    this.publish({ ...this.state, phase: 'loading', error: null });
    try {
      // read() alone says nothing about an outstanding write. Fail closed first.
      const receipt = await this.port.reconcile(scope);
      if (!this.accepts(ticket)) return;
      if (receipt) {
        this.adopt(receipt);
        return;
      }
      const current = await this.port.read(scope);
      if (!this.accepts(ticket)) return;
      if (!this.valid(current)) throw Error('Invalid definition scope');
      const baseline = freezeDefinition(current);
      this.publish({
        ...initial(this.state.scope),
        phase: 'editing',
        current: baseline,
        persona: baseline.persona,
        rules: baseline.rules,
        ruleKeys: baseline.rules.map(() => ++this.ruleKey),
        receiptKnown: true,
      });
    } catch {
      if (this.accepts(ticket)) this.publish({ ...this.state, phase: 'error', error: 'load' });
    } finally {
      if (ticket.generation === this.generation) this.loading = false;
    }
  }
  dirty(observed: DefinitionState = this.state) {
    return (
      observed === this.state &&
      !!observed.current &&
      !sameText(observed.current, { persona: observed.persona, rules: [...observed.rules] })
    );
  }
  edit(persona: string, rules: readonly string[]) {
    this.change(
      persona,
      rules,
      rules.map((_, index) => this.state.ruleKeys[index] ?? ++this.ruleKey),
    );
  }
  private change(persona: string, rules: readonly string[], ruleKeys: readonly number[]) {
    if (
      !this.scope ||
      !['editing', 'previewing', 'review'].includes(this.state.phase) ||
      this.state.current?.status === 'Revoked'
    )
      return;
    this.revision++;
    this.publish({
      ...this.state,
      phase: 'editing',
      persona,
      rules: Object.freeze([...rules]),
      ruleKeys: Object.freeze([...ruleKeys]),
      review: null,
      error: null,
    });
  }
  setPersona(persona: string) {
    this.change(persona, this.state.rules, this.state.ruleKeys);
  }
  setRule(key: number, value: string) {
    const index = this.state.ruleKeys.indexOf(key);
    if (index < 0) return;
    const rules = [...this.state.rules];
    rules[index] = value;
    this.change(this.state.persona, rules, this.state.ruleKeys);
  }
  addRule() {
    if (this.state.rules.length < 8)
      this.change(
        this.state.persona,
        [...this.state.rules, ''],
        [...this.state.ruleKeys, ++this.ruleKey],
      );
  }
  removeRule(key: number) {
    const index = this.state.ruleKeys.indexOf(key);
    if (index < 0) return;
    this.change(
      this.state.persona,
      this.state.rules.filter((_, i) => i !== index),
      this.state.ruleKeys.filter((_, i) => i !== index),
    );
  }
  async reload(observed: DefinitionTicket) {
    if (
      !this.scope ||
      !this.accepts(observed) ||
      !['editing', 'previewing', 'review'].includes(this.state.phase)
    )
      return;
    this.activate(this.scope);
    await this.load();
  }
  async preview() {
    if (
      !this.scope ||
      this.state.phase !== 'editing' ||
      !this.state.current ||
      this.state.current.status === 'Revoked' ||
      !this.dirty()
    )
      return;
    const scope = this.scope,
      ticket = this.ticket()!,
      baseline = this.state.current,
      persona = this.state.persona,
      rules = [...this.state.rules];
    this.publish({ ...this.state, phase: 'previewing', error: null });
    try {
      const review = await this.port.preview(scope, baseline, persona, rules);
      if (!this.accepts(ticket)) return;
      if (
        !this.validReview(review) ||
        review.current.status === 'Revoked' ||
        review.current.version !== baseline.version ||
        review.current.definition_sha256 !== baseline.definition_sha256 ||
        !sameText(review.current, baseline) ||
        !sameText(review.proposed, { persona, rules })
      )
        throw Error('Review changed');
      this.publish({ ...this.state, phase: 'review', review: freezeReview(review) });
    } catch {
      if (this.accepts(ticket)) this.publish({ ...this.state, phase: 'editing', error: 'preview' });
    }
  }
  confirmation(observed: DefinitionState = this.state): DefinitionConfirmation | null {
    const ticket = this.ticket(observed);
    return ticket && observed.phase === 'review' && observed.review
      ? { ...ticket, digest: observed.review.preview_digest }
      : null;
  }
  async apply(observed: DefinitionConfirmation) {
    if (
      !this.scope ||
      !this.accepts(observed) ||
      this.state.phase !== 'review' ||
      !this.state.review ||
      observed.digest !== this.state.review.preview_digest
    )
      return;
    const scope = this.scope,
      ticket = this.ticket()!,
      review = this.state.review;
    this.publish({ ...this.state, phase: 'submitting', receiptKnown: false, error: null });
    try {
      const receipt = await this.port.update(scope, review);
      if (this.accepts(ticket)) this.adopt(receipt, review);
    } catch {
      // A thrown or malformed response cannot restore Apply: dispatch may be durable.
      if (this.accepts(ticket))
        this.publish({
          ...this.state,
          phase: 'held',
          receipt: null,
          receiptKnown: false,
          current: null,
        });
    }
  }
  async reconcile() {
    if (
      !this.scope ||
      this.state.phase !== 'held' ||
      this.state.reconciling ||
      this.state.dismissing
    )
      return;
    const scope = this.scope,
      ticket = this.ticket()!;
    this.publish({ ...this.state, reconciling: true, error: null });
    try {
      const receipt = await this.port.reconcile(scope);
      if (!this.accepts(ticket)) return;
      if (receipt) this.adopt(receipt);
      else this.publish({ ...this.state, receipt: null, receiptKnown: true, reconciling: false });
    } catch {
      if (this.accepts(ticket))
        this.publish({ ...this.state, reconciling: false, error: 'reconcile' });
    }
  }
  dismissal(): DefinitionDismissal | null {
    const ticket = this.ticket();
    return ticket &&
      this.state.phase === 'held' &&
      this.state.review &&
      this.state.receiptKnown &&
      !this.state.reconciling &&
      !this.state.dismissing
      ? {
          ...ticket,
          digest: this.state.review.preview_digest,
          receipt: this.state.receipt?.receipt_id ?? null,
        }
      : null;
  }
  async dismiss(observed: DefinitionDismissal) {
    const current = this.dismissal();
    if (
      !this.scope ||
      !current ||
      !this.accepts(observed) ||
      observed.digest !== current.digest ||
      observed.receipt !== current.receipt
    )
      return;
    const scope = this.scope,
      ticket = this.ticket()!,
      receipt = this.state.receipt;
    this.publish({ ...this.state, dismissing: true, error: null });
    try {
      let check = true;
      if (receipt) {
        try {
          check = !(await this.port.dismiss(scope, receipt));
        } catch {
          /* A lost dismissal ACK needs a read, never another write. */
        }
      }
      if (!this.accepts(ticket)) return;
      if (check) {
        const held = await this.port.reconcile(scope);
        if (!this.accepts(ticket)) return;
        if (held) {
          this.adopt(held);
          this.publish({ ...this.state, dismissing: false, error: 'dismiss' });
          return;
        }
      }
      this.activate(scope);
      await this.load();
    } catch {
      if (this.accepts(ticket))
        this.publish({ ...this.state, dismissing: false, error: 'dismiss' });
    }
  }
}
