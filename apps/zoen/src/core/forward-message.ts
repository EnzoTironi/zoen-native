import type { ForwardReview as CanonicalReview } from './generated/ForwardReview';
import type { ForwardScope } from './generated/ForwardScope';
import type { ForwardSource } from './generated/ForwardSource';
import { homeOwnerKey, type HomeOwner } from '../home/preferences.ts';

export type ForwardMessageScope = Readonly<{
  owner: HomeOwner;
  spaceId: string;
  messageId: string;
  sessionEpoch?: number;
}>;
export type ForwardMessagePort = {
  owner(): HomeOwner | null;
  spaces(): ReadonlySet<string>;
  epoch?(): number;
  writable?(targetId: string): boolean;
  status(source: ForwardSource): Promise<CanonicalReview | null>;
  review(scope: ForwardScope): Promise<CanonicalReview>;
  confirm(
    scope: ForwardScope,
    operationId: string,
    sourceRevision: string,
  ): Promise<CanonicalReview>;
  dismiss(
    scope: ForwardScope,
    operationId: string,
    acceptedMessageId: string | null,
  ): Promise<void>;
};
export type ForwardMessageReview = Readonly<{
  targetId: CanonicalReview['target_space'];
  message: CanonicalReview['message'];
  operationId: CanonicalReview['operation_id'];
  sourceRevision: CanonicalReview['source_revision'];
  acceptedMessageId: CanonicalReview['accepted_message_id'];
}>;
export type ForwardMessageCallbacks = {
  busy(value: boolean): void;
  error(reason: unknown | null): void;
  sent(targetId: string): void;
  reviewed?(value: ForwardMessageReview | null): void;
};
export class ForwardMessageUnavailable extends Error {
  constructor() {
    super('Message unavailable');
  }
}
export class ForwardMessageChanged extends ForwardMessageUnavailable {}
type Attempt = { epoch: number; targetId: string | null };

/** UI focus owns presentation only. The Core owns operation IDs, snapshots and
 * accepted receipts; transport failures never create another forwarding intent. */
export class ForwardMessageSession {
  private focused = false;
  private disposed = false;
  private epoch = 0;
  private attempt: Attempt | null = null;
  private selected: ForwardMessageReview | null = null;
  private readonly scope: ForwardMessageScope | null;
  private readonly port: ForwardMessagePort;
  private readonly callbacks: ForwardMessageCallbacks;
  constructor(
    scope: ForwardMessageScope | null,
    port: ForwardMessagePort,
    callbacks: ForwardMessageCallbacks,
  ) {
    this.scope = scope && { ...scope, owner: { ...scope.owner } };
    this.port = port;
    this.callbacks = callbacks;
  }
  private available(targetId: string | null = null) {
    const scope = this.scope;
    return Boolean(
      scope &&
      scope.owner.mode === 'account' &&
      !this.disposed &&
      this.focused &&
      homeOwnerKey(this.port.owner()) === homeOwnerKey(scope.owner) &&
      (scope.sessionEpoch === undefined || this.port.epoch?.() === scope.sessionEpoch) &&
      this.port.spaces().has(scope.spaceId) &&
      (targetId === null ||
        (this.port.spaces().has(targetId) && this.port.writable?.(targetId) !== false)),
    );
  }
  private source(): ForwardSource {
    if (!this.scope) throw new ForwardMessageUnavailable();
    return {
      expected_identity_id: this.scope.owner.identityId,
      expected_device_id: this.scope.owner.deviceId,
      source_space: this.scope.spaceId,
      message_id: this.scope.messageId,
    };
  }
  private target(target_space: string): ForwardScope {
    return { ...this.source(), target_space };
  }
  private current(attempt: Attempt) {
    return (
      this.attempt === attempt && this.epoch === attempt.epoch && this.available(attempt.targetId)
    );
  }
  private project(value: CanonicalReview | null) {
    this.selected =
      value &&
      Object.freeze({
        targetId: value.target_space,
        message: value.message,
        operationId: value.operation_id,
        sourceRevision: value.source_revision,
        acceptedMessageId: value.accepted_message_id,
      });
    this.callbacks.reviewed?.(this.selected);
  }
  async focus() {
    this.disposed = false;
    this.focused = true;
    ++this.epoch;
    this.attempt = null;
    this.project(null);
    this.callbacks.error(null);
    this.callbacks.busy(false);
    await this.recover();
  }
  blur() {
    this.focused = false;
    ++this.epoch;
    this.attempt = null;
    this.selected = null;
    this.callbacks.reviewed?.(null);
    // Never dismiss the Core record here: a confirmation may already be signed.
  }
  dispose() {
    this.blur();
    this.disposed = true;
  }
  refresh() {
    if (!this.available()) {
      this.selected = null;
      this.callbacks.reviewed?.(null);
    }
    if (!this.attempt || this.current(this.attempt)) return;
    ++this.epoch;
    this.attempt = null;
    if (this.focused && !this.disposed) {
      this.callbacks.busy(false);
      this.callbacks.error(null);
    }
  }
  private async run(targetId: string | null, action: (attempt: Attempt) => Promise<void>) {
    if (this.attempt || !this.available(targetId)) return;
    const attempt = { epoch: this.epoch, targetId };
    this.attempt = attempt;
    this.callbacks.busy(true);
    this.callbacks.error(null);
    try {
      await action(attempt);
    } catch (reason) {
      if (this.current(attempt)) this.callbacks.error(reason);
    } finally {
      const publish = this.current(attempt);
      if (this.attempt === attempt) this.attempt = null;
      if (publish) this.callbacks.busy(false);
    }
  }
  async recover() {
    await this.run(null, async (attempt) => {
      const value = await this.port.status(this.source());
      if (this.current(attempt)) this.project(value);
    });
  }
  async prepare(targetId: string) {
    await this.run(targetId, async (attempt) => {
      // A retained operation, including an unknown acceptance, wins over any
      // new destination selection. Reading it never publishes another message.
      const retained = await this.port.status(this.source());
      if (!this.current(attempt)) return;
      const value = retained ?? (await this.port.review(this.target(targetId)));
      if (this.current(attempt)) this.project(value);
    });
  }
  async confirm(review: ForwardMessageReview) {
    if (review !== this.selected || review.acceptedMessageId !== null) return;
    await this.run(review.targetId, async (attempt) => {
      try {
        const value = await this.port.confirm(
          this.target(review.targetId),
          review.operationId,
          review.sourceRevision,
        );
        if (!this.current(attempt)) return;
        this.project(value);
        if (value.accepted_message_id !== null) {
          // Only this live, observed confirmation can ACK its exact output.
          // A failed cleanup never turns accepted into retryable/unsent.
          try {
            await this.port.dismiss(
              this.target(value.target_space),
              value.operation_id,
              value.accepted_message_id,
            );
          } catch {
            /* Core retains recovery if cleanup was not accepted. */
          }
          if (this.current(attempt)) this.callbacks.sent(value.target_space);
        }
      } catch (reason) {
        if (!this.current(attempt)) return;
        // Reconcile exact operation first. An unreadable status retains its
        // frozen review; neither failure permits a fresh operation or auto-send.
        const retained = await this.port.status(this.source());
        if (!this.current(attempt)) return;
        if (retained) this.project(retained);
        if (retained?.accepted_message_id) this.callbacks.sent(retained.target_space);
        else throw reason;
      }
    });
  }
  async cancelReview() {
    const review = this.selected;
    if (!review) return;
    await this.run(null, async (attempt) => {
      try {
        await this.port.dismiss(
          this.target(review.targetId),
          review.operationId,
          review.acceptedMessageId,
        );
        if (!this.current(attempt)) return;
        this.project(null);
      } catch (reason) {
        if (!this.current(attempt)) return;
        const retained = await this.port.status(this.source());
        if (this.current(attempt)) this.project(retained);
        if (!retained || retained.accepted_message_id) return;
        throw reason;
      }
    });
  }
}
