/** A focused search owns its pending reads and navigation, never a later route. */
export class GlobalSearchActions {
  private active = false;
  private generation = 0;
  private retryFailedRead: (() => Promise<void>) | null = null;
  private state = { pending: false, error: null as string | null };
  private listeners = new Set<() => void>();
  private readonly current: () => boolean;
  readonly query: string;
  constructor(current: () => boolean, query = '') {
    this.current = current;
    this.query = query;
  }
  isCurrent() {
    return this.active && this.current();
  }
  retry() {
    if (this.isCurrent() && !this.state.pending) return this.retryFailedRead?.();
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(pending: boolean, error: string | null) {
    this.state = { pending, error };
    for (const listener of this.listeners) listener();
  }
  focus() {
    this.retryFailedRead = null;
    this.active = true;
    this.generation++;
    this.publish(false, null);
  }
  blur() {
    this.retryFailedRead = null;
    this.active = false;
    this.generation++;
  }
  /** Observe access loss immediately, so restoring it cannot revive an old read. */
  refresh() {
    if (!this.current()) this.blur();
  }
  async run<T>(read: () => Promise<T>, complete: (value: T) => void, fallback: string) {
    if (!this.active || !this.current() || this.state.pending) return;
    const generation = this.generation;
    const valid = () => this.active && generation === this.generation && this.current();
    this.retryFailedRead = null;
    this.publish(true, null);
    let error: string | null = null;
    try {
      const value = await read();
      if (valid()) complete(value);
    } catch (reason) {
      if (valid()) {
        this.retryFailedRead = () => this.run(read, complete, fallback);
        error = reason instanceof Error ? reason.message : fallback;
      }
    } finally {
      if (valid()) this.publish(false, error);
    }
  }
}
