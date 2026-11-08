import type { MessagePinScope } from '../core/generated/MessagePinScope';
import type { MessagePinReview } from '../core/generated/MessagePinReview';
import type { MessagePinEdit } from '../core/generated/MessagePinEdit';
import type { MessagePinPage } from '../core/generated/MessagePinPage';
import type { MessagePinResult } from '../core/generated/MessagePinResult';
export type PinsState = MessagePinPage & {
  generation: number;
  loading: boolean;
  error: boolean;
  actionError: boolean;
  pending: boolean;
  review: MessagePinReview | null;
};
export interface PinsPort {
  current(): boolean;
  page(scope: MessagePinScope, cursor: string | null): Promise<MessagePinPage>;
  review(scope: MessagePinScope, message: string): Promise<MessagePinReview>;
  write(input: MessagePinEdit): Promise<MessagePinResult>;
  operationId(): string;
}
const empty = (generation: number): PinsState => ({
  generation,
  loading: false,
  error: false,
  actionError: false,
  pending: false,
  review: null,
  entries: [],
  total: 0,
  next: null,
  revision: '',
});
/** Transient projections only. Signed annotations/outbox own state and exact write receipts. */
export class MessagePinsSession {
  private state = empty(0);
  private active = false;
  private turn = 0;
  private readFlight: object | null = null;
  private refreshQueued = false;
  private listeners = new Set<() => void>();
  private readonly scope: MessagePinScope;
  private readonly message: string | null;
  private readonly port: PinsPort;
  constructor(scope: MessagePinScope, message: string | null, port: PinsPort) {
    this.scope = Object.freeze({ ...scope });
    this.message = message;
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (callback: () => void) => {
    this.listeners.add(callback);
    return () => {
      this.listeners.delete(callback);
    };
  };
  private publish(patch: Partial<PinsState>) {
    this.state = { ...this.state, ...patch };
    for (const callback of this.listeners) callback();
  }
  current(generation: number) {
    return this.active && generation === this.state.generation && this.port.current();
  }
  focus() {
    this.blur();
    this.active = true;
    void this.refresh();
  }
  blur() {
    this.active = false;
    this.turn++;
    this.readFlight = null;
    this.refreshQueued = false;
    this.state = empty(this.state.generation + 1);
    for (const callback of this.listeners) callback();
  }
  check() {
    if (this.active && !this.port.current()) this.blur();
  }
  refresh() {
    return this.load(false, this.state.generation);
  }
  more(generation: number) {
    return this.load(true, generation);
  }
  private matches(review: MessagePinReview) {
    return (
      review.entry.id === this.message &&
      Object.keys(this.scope).every(
        (key) =>
          review.scope[key as keyof MessagePinScope] === this.scope[key as keyof MessagePinScope],
      )
    );
  }
  private async load(more: boolean, generation: number) {
    if (
      !this.current(generation) ||
      this.state.pending ||
      (more && (!this.state.next || this.state.loading))
    )
      return;
    if (this.readFlight) {
      if (!more) {
        this.turn++;
        this.refreshQueued = true;
      }
      return;
    }
    const attempt = {};
    this.readFlight = attempt;
    const turn = ++this.turn,
      previous = this.state;
    this.publish({ loading: true, error: false });
    try {
      if (this.message) {
        const review = await this.port.review(this.scope, this.message);
        if (!this.current(generation) || turn !== this.turn) return;
        if (!this.matches(review)) throw Error('pin-review-mismatch');
        this.publish({ review, loading: false });
      } else {
        const page = await this.port.page(this.scope, more ? previous.next : null);
        if (!this.current(generation) || turn !== this.turn) return;
        if (
          page.entries.some((entry) => entry.deleted || !entry.pinned || !('Message' in entry.kind))
        )
          throw Error('pin-page-invalid');
        if (more && page.revision !== previous.revision) {
          void this.refresh();
          return;
        }
        const retain =
          !more &&
          page.revision === previous.revision &&
          previous.entries.length > page.entries.length;
        const entries = more
          ? [...previous.entries, ...page.entries]
          : retain
            ? previous.entries
            : page.entries;
        if (new Set(entries.map((entry) => entry.id)).size !== entries.length)
          throw Error('pin-page-duplicate');
        this.publish({
          ...page,
          entries,
          next: retain ? previous.next : page.next,
          loading: false,
        });
      }
    } catch {
      if (this.current(generation) && turn === this.turn)
        this.publish({ loading: false, error: true });
    } finally {
      if (this.readFlight === attempt) {
        this.readFlight = null;
        if (this.refreshQueued && this.current(generation)) {
          this.refreshQueued = false;
          void this.refresh();
        }
      }
    }
  }
  async toggle(generation: number) {
    const review = this.state.review;
    if (
      !this.current(generation) ||
      this.state.pending ||
      this.state.loading ||
      !review?.can_pin ||
      !review.entry.text_revision ||
      !this.matches(review)
    )
      return;
    this.turn++;
    this.publish({ pending: true, actionError: false });
    try {
      const input: MessagePinEdit = {
        scope: this.scope,
        message_id: review.entry.id,
        change: {
          operation_id: this.port.operationId(),
          expected_message_revision: review.entry.text_revision,
          expected_pin_revision: review.pin_revision,
          pinned: !review.pinned,
        },
      };
      const result = await this.port.write(input);
      if (!this.current(generation)) return;
      if (!this.matches(result.review)) throw Error('pin-result-mismatch');
      this.publish({ review: result.review });
    } catch {
      if (this.current(generation)) this.publish({ actionError: true });
    } finally {
      if (this.current(generation)) {
        this.publish({ pending: false });
        void this.refresh();
      }
    }
  }
}
