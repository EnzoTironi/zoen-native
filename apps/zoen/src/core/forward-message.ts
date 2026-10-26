import type { TimelineEntry } from './types';
import { messageCanInteract } from './message-presentation.ts';
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
  message(spaceId: string, messageId: string): Promise<TimelineEntry>;
  forward(spaceId: string, messageId: string, targetId: string): Promise<unknown>;
};
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
export type ForwardMessageReview = Readonly<{ targetId: string; message: TimelineEntry }>;
function contentKey(message: TimelineEntry) {
  // Delivery, reactions and read counters do not change the content being sent.
  return JSON.stringify([
    message.id,
    message.author?.id,
    message.kind,
    message.voice,
    message.media,
    message.forwarded,
    message.deleted,
  ]);
}

type Attempt = { epoch: number; targetId: string };

/** A focus owns navigation, not the delivery of an already accepted forward. */
export class ForwardMessageSession {
  private readonly scope: ForwardMessageScope | null;
  private readonly port: ForwardMessagePort;
  private readonly callbacks: ForwardMessageCallbacks;
  private focused = false;
  private disposed = false;
  private epoch = 0;
  private attempt: Attempt | null = null;
  private review: ForwardMessageReview | null = null;
  private reviewKey: string | null = null;

  constructor(
    scope: ForwardMessageScope | null,
    port: ForwardMessagePort,
    callbacks: ForwardMessageCallbacks,
  ) {
    this.scope = scope && { ...scope, owner: { ...scope.owner } };
    this.port = port;
    this.callbacks = callbacks;
  }
  private available(targetId: string) {
    const scope = this.scope;
    if (
      !scope ||
      this.disposed ||
      !this.focused ||
      homeOwnerKey(this.port.owner()) !== homeOwnerKey(scope.owner) ||
      (scope.sessionEpoch !== undefined && this.port.epoch?.() !== scope.sessionEpoch) ||
      this.port.writable?.(targetId) === false
    )
      return false;
    const spaces = this.port.spaces();
    return spaces.has(scope.spaceId) && spaces.has(targetId);
  }
  private current(attempt: Attempt) {
    return (
      this.attempt === attempt && this.epoch === attempt.epoch && this.available(attempt.targetId)
    );
  }
  focus() {
    this.disposed = false;
    this.focused = true;
    ++this.epoch;
    this.attempt = null;
    this.cancelReview();
    this.callbacks.busy(false);
    this.callbacks.error(null);
  }
  blur() {
    this.focused = false;
    ++this.epoch;
    this.attempt = null;
    this.cancelReview();
  }
  dispose() {
    this.blur();
    this.disposed = true;
  }
  /** Release invalid presentation at the access-change event, not on late completion. */
  refresh() {
    if (this.review && !this.available(this.review.targetId)) this.cancelReview();
    if (!this.attempt || this.current(this.attempt)) return;
    ++this.epoch;
    this.attempt = null;
    if (this.focused && !this.disposed) {
      this.callbacks.busy(false);
      this.callbacks.error(null);
    }
  }
  cancelReview() {
    this.review = null;
    this.reviewKey = null;
    this.callbacks.reviewed?.(null);
  }
  async prepare(targetId: string) {
    if (this.attempt || !this.available(targetId) || !this.scope) return;
    this.cancelReview();
    const attempt = { epoch: this.epoch, targetId };
    this.attempt = attempt;
    this.callbacks.busy(true);
    this.callbacks.error(null);
    try {
      const message = await this.port.message(this.scope.spaceId, this.scope.messageId);
      if (!this.current(attempt)) return;
      if (message.id !== this.scope.messageId || !messageCanInteract(message))
        throw new ForwardMessageUnavailable();
      // Keep the reviewed projection independent of mutable snapshot objects.
      this.review = Object.freeze({
        targetId,
        message: JSON.parse(JSON.stringify(message)) as TimelineEntry,
      });
      this.reviewKey = contentKey(message);
      this.callbacks.reviewed?.(this.review);
    } catch (reason) {
      if (this.current(attempt)) this.callbacks.error(reason);
    } finally {
      const publish = this.current(attempt);
      if (this.attempt === attempt) this.attempt = null;
      if (publish) this.callbacks.busy(false);
    }
  }
  async confirm(review: ForwardMessageReview) {
    if (review !== this.review || !this.available(review.targetId)) return;
    await this.send(review.targetId, this.reviewKey);
  }
  async send(targetId: string, reviewedKey: string | null = null): Promise<void> {
    if (this.attempt || !this.available(targetId) || !this.scope) return;
    const scope = this.scope;
    const attempt = { epoch: this.epoch, targetId };
    this.attempt = attempt;
    this.callbacks.busy(true);
    this.callbacks.error(null);
    try {
      // Rendered timeline entries may have changed while the picker was open.
      const message = await this.port.message(scope.spaceId, scope.messageId);
      if (!this.current(attempt)) return;
      if (message.id !== scope.messageId || !messageCanInteract(message))
        throw new ForwardMessageUnavailable();
      if (reviewedKey !== null && contentKey(message) !== reviewedKey) {
        this.cancelReview();
        throw new ForwardMessageChanged();
      }
      await this.port.forward(scope.spaceId, scope.messageId, targetId);
      if (this.current(attempt)) {
        this.cancelReview();
        this.callbacks.sent(targetId);
      }
    } catch (reason) {
      if (this.current(attempt)) this.callbacks.error(reason);
    } finally {
      const publish = this.current(attempt);
      if (this.attempt === attempt) this.attempt = null;
      if (publish) this.callbacks.busy(false);
    }
  }
}
