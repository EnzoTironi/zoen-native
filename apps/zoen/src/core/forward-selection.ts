import type { ForwardBatchReview } from './generated/ForwardBatchReview';
import type { ForwardBatchSource } from './generated/ForwardBatchSource';
import type { ForwardBatchScope } from './generated/ForwardBatchScope';
import { homeOwnerKey, type HomeOwner } from '../home/preferences.ts';
import {
  ForwardMessageChanged,
  ForwardMessageUnavailable,
  type ForwardMessageReview,
} from './forward-message.ts';

export type ForwardSelectionScope = Readonly<{
  owner: HomeOwner;
  spaceId: string;
  messageIds: readonly string[];
  sessionEpoch?: number;
}>;
export type ForwardSelectionReview = Readonly<{
  batchId: string;
  revision: string;
  targetId: string;
  messages: readonly ForwardMessageReview[];
  ready: boolean;
  completed: boolean;
  skippedIds: readonly string[];
  error: string | null;
}>;
export type ForwardSelectionPort = {
  owner(): HomeOwner | null;
  spaces(): ReadonlySet<string>;
  epoch?(): number;
  writable?(targetId: string): boolean;
  status(source: ForwardBatchSource): Promise<ForwardBatchReview | null>;
  review(scope: ForwardBatchScope): Promise<ForwardBatchReview>;
  confirm(
    source: ForwardBatchSource,
    batchId: string,
    revision: string,
  ): Promise<ForwardBatchReview>;
  finish(source: ForwardBatchSource, batchId: string): Promise<ForwardBatchReview>;
  abandon(
    source: ForwardBatchSource,
    batchId: string,
    revision: string,
  ): Promise<ForwardBatchReview>;
};
export type ForwardSelectionCallbacks = {
  busy(value: boolean): void;
  error(reason: unknown | null): void;
  reviewed?(review: ForwardSelectionReview | null): void;
  sent(targetId: string): void;
};
type Attempt = { epoch: number; targetId: string | null };
function immutable<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) immutable(child);
    Object.freeze(value);
  }
  return value;
}

/** Presentation owns the current review only. The Core owns the exact set,
 * revisions, per-item outcomes and durable completion across cold recovery. */
export class ForwardSelectionSession {
  private readonly scope: ForwardSelectionScope | null;
  private readonly port: ForwardSelectionPort;
  private readonly callbacks: ForwardSelectionCallbacks;
  private focused = false;
  private disposed = false;
  private epoch = 0;
  private attempt: Attempt | null = null;
  private selected: ForwardSelectionReview | null = null;
  private notifiedBatch: string | null = null;
  constructor(
    scope: ForwardSelectionScope | null,
    port: ForwardSelectionPort,
    callbacks: ForwardSelectionCallbacks,
  ) {
    this.scope =
      scope && scope.messageIds.length && new Set(scope.messageIds).size === scope.messageIds.length
        ? { ...scope, owner: { ...scope.owner }, messageIds: Object.freeze([...scope.messageIds]) }
        : null;
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
  private source(): ForwardBatchSource {
    if (!this.scope) throw new ForwardMessageUnavailable();
    return {
      expected_identity_id: this.scope.owner.identityId,
      expected_device_id: this.scope.owner.deviceId,
      source_space: this.scope.spaceId,
      message_ids: [...this.scope.messageIds],
    };
  }
  private current(attempt: Attempt) {
    return (
      this.attempt === attempt && this.epoch === attempt.epoch && this.available(attempt.targetId)
    );
  }
  private project(value: ForwardBatchReview | null) {
    if (!value) {
      this.selected = null;
    } else {
      const ids = value.messages.map((item) => item.message.id);
      if (
        ids.length !== this.scope!.messageIds.length ||
        new Set(ids).size !== ids.length ||
        this.scope!.messageIds.some((id) => !ids.includes(id)) ||
        new Set(value.skipped_message_ids).size !== value.skipped_message_ids.length ||
        value.skipped_message_ids.some(
          (id) =>
            !ids.includes(id) ||
            value.messages.some(
              (item) => item.message.id === id && item.accepted_message_id !== null,
            ),
        )
      ) {
        this.expire();
        throw new ForwardMessageChanged();
      }
      this.selected = immutable({
        batchId: value.batch_id,
        revision: value.review_revision,
        targetId: value.target_space,
        ready: value.ready,
        completed: value.completed,
        error: value.error,
        skippedIds: [...value.skipped_message_ids],
        messages: value.messages.map((item) => ({
          targetId: item.target_space,
          // Driver DTOs are JSON values; avoid a structuredClone runtime requirement on Hermes.
          message: JSON.parse(JSON.stringify(item.message)) as typeof item.message,
          operationId: item.operation_id,
          sourceRevision: item.source_revision,
          acceptedMessageId: item.accepted_message_id,
        })),
      });
    }
    this.callbacks.reviewed?.(this.selected);
    if (value?.error) this.callbacks.error(new Error(value.error));
  }
  private expire() {
    if (!this.selected) return;
    this.selected = Object.freeze({ ...this.selected, ready: false });
    this.callbacks.reviewed?.(this.selected);
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
  async focus() {
    this.disposed = false;
    this.focused = true;
    ++this.epoch;
    this.attempt = null;
    this.project(null);
    this.callbacks.busy(false);
    this.callbacks.error(null);
    await this.recover();
  }
  blur() {
    this.focused = false;
    ++this.epoch;
    this.attempt = null;
    this.project(null);
  }
  dispose() {
    this.blur();
    this.disposed = true;
  }
  refresh() {
    if (!this.available()) {
      ++this.epoch;
      this.attempt = null;
      this.project(null);
      if (this.focused && !this.disposed) this.callbacks.busy(false);
    } else if (this.attempt && !this.current(this.attempt)) {
      ++this.epoch;
      this.attempt = null;
      this.expire();
      this.callbacks.busy(false);
    }
  }
  /** Retire UI action references before the caller opens the recipient picker. */
  clearReview() {
    ++this.epoch;
    this.attempt = null;
    this.selected = null;
    if (this.focused && !this.disposed) this.callbacks.busy(false);
  }
  async recover() {
    await this.run(null, async (attempt) => {
      const value = await this.port.status(this.source());
      if (this.current(attempt)) this.project(value);
    });
  }
  async prepare(targetId: string) {
    await this.run(targetId, async (attempt) => {
      this.project(null);
      const value = await this.port.review({ source: this.source(), target_space: targetId });
      if (!this.current(attempt)) return;
      if (value.target_space !== targetId) throw new ForwardMessageChanged();
      this.project(value);
    });
  }
  async confirm(review: ForwardSelectionReview) {
    if (review !== this.selected || !review.ready || review.completed) return;
    await this.run(review.targetId, async (attempt) => {
      try {
        const value = await this.port.confirm(this.source(), review.batchId, review.revision);
        if (!this.current(attempt)) return;
        if (value.batch_id !== review.batchId || value.target_space !== review.targetId)
          throw new ForwardMessageChanged();
        this.project(value);
      } catch (reason) {
        if (!this.current(attempt)) return;
        // Read the same durable batch after an unknown transport acceptance.
        // A recovered result never triggers another confirm or auto-completion.
        this.expire();
        const retained = await this.port.status(this.source());
        if (!this.current(attempt)) return;
        if (retained?.batch_id === review.batchId) this.project(retained);
        throw reason;
      }
    });
  }
  async finish(review: ForwardSelectionReview) {
    if (
      review !== this.selected ||
      review.completed ||
      !review.messages.length ||
      review.messages.some(
        (value) =>
          value.acceptedMessageId === null && !review.skippedIds.includes(value.message.id),
      ) ||
      this.notifiedBatch === review.batchId
    )
      return;
    // Completion acknowledges existing receipts and requires source authority;
    // it never requires write/membership in an old accepted destination.
    await this.run(null, async (attempt) => {
      try {
        const value = await this.port.finish(this.source(), review.batchId);
        if (!this.current(attempt)) return;
        if (value.batch_id !== review.batchId || !value.completed)
          throw new ForwardMessageChanged();
        this.project(value);
        this.notifiedBatch = value.batch_id;
        this.callbacks.sent(value.target_space);
      } catch (reason) {
        if (!this.current(attempt)) return;
        const retained = await this.port.status(this.source());
        if (!this.current(attempt)) return;
        if (retained?.batch_id === review.batchId) this.project(retained);
        throw reason;
      }
    });
  }
  async abandon(review: ForwardSelectionReview) {
    if (
      review !== this.selected ||
      review.completed ||
      !review.messages.some(
        (value) =>
          value.acceptedMessageId === null && !review.skippedIds.includes(value.message.id),
      )
    )
      return;
    await this.run(null, async (attempt) => {
      try {
        const value = await this.port.abandon(this.source(), review.batchId, review.revision);
        if (!this.current(attempt)) return;
        if (value.batch_id !== review.batchId || !value.completed)
          throw new ForwardMessageChanged();
        this.project(value);
        // Keep the terminal accepted/skipped summary visible. Abandon is not Send.
      } catch (reason) {
        if (!this.current(attempt)) return;
        const retained = await this.port.status(this.source());
        if (!this.current(attempt)) return;
        if (retained?.batch_id === review.batchId) this.project(retained);
        throw reason;
      }
    });
  }
}
