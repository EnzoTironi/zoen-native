import type {
  MessageTextSelectionBinding,
  MessageTextSelectionHandler,
} from '../../modules/zoen-core/src/ZoenMessageContext.types';

type Bound = { gestureId: string; current(): boolean; handler: MessageTextSelectionHandler };
type Lease = { requestId: string; owner: Bound };
/** UI-only lease for the existing extracted subtree. No message/draft persistence. */
export class MessageTextSelectionSession {
  private bound: Bound | null = null;
  private lease: Lease | null = null;
  private state = { active: false, pending: false, error: false };
  private readonly listeners = new Set<() => void>();
  private finish: (requestId: string, result: string) => Promise<void>;
  constructor(
    finish: (requestId: string, result: string) => Promise<void> = async () => {
      throw new Error('Selection unavailable');
    },
  ) {
    this.finish = finish;
  }
  connect(finish: (requestId: string, result: string) => Promise<void>) {
    this.finish = finish;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(active: boolean, pending = false, error = false) {
    if (
      this.state.active === active &&
      this.state.pending === pending &&
      this.state.error === error
    )
      return;
    this.state = { active, pending, error };
    for (const listener of this.listeners) listener();
  }
  binding(gestureId: string, current: () => boolean): MessageTextSelectionBinding {
    return {
      bind: (handler) => {
        const owner = { gestureId, current, handler };
        this.retire();
        this.bound = owner;
        return () => {
          if (this.bound === owner) {
            this.retire();
            this.bound = null;
          }
        };
      },
    };
  }
  phase(gestureId: string, requestId: string, active: boolean) {
    if (!active) {
      if (this.lease?.requestId === requestId && this.lease.owner.gestureId === gestureId)
        this.retire();
      return;
    }
    const owner = this.bound;
    if (!owner || owner.gestureId !== gestureId || !owner.current()) return;
    if (this.lease?.requestId === requestId) return;
    this.lease = { requestId, owner };
    owner.handler.started();
    this.publish(true);
  }
  retire() {
    this.lease = null;
    this.publish(false);
  }
  quote = async (text: string, ranges?: string) => {
    const lease = this.lease;
    if (!lease || this.bound !== lease.owner || !lease.owner.current() || this.state.pending)
      return;
    if (!ranges) {
      this.publish(true, false, true);
      return;
    }
    this.publish(true, true);
    const current = () =>
      this.lease === lease && this.bound === lease.owner && lease.owner.current();
    try {
      const payload = await lease.owner.handler.review(text, ranges);
      if (!current()) return;
      if (!payload || payload.length > 65536) throw new Error('Invalid quote result');
      await this.finish(lease.requestId, payload);
    } catch {
      if (current()) this.publish(true, false, true);
    } finally {
      if (current() && this.state.pending) this.publish(true);
    }
  };
}
