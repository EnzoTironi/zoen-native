import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { SpaceSummary } from '../core/generated/SpaceSummary';

type Port = {
  scope(): AccountScope | null;
  summary(): SpaceSummary | undefined;
  ensure(scope: AccountScope): Promise<SpaceSummary>;
  open(id: string): void;
};
type State = {
  scope: string;
  revision: number;
  phase: 'opening' | 'waiting' | 'failed' | 'unavailable';
};

/** Only transient entry progress lives here; the canonical snapshot owns the saved chat. */
export class SavedMessagesSession {
  private owner: AccountScope | null = null;
  private generation = 0;
  private pending = false;
  private opened = false;
  private state: State = { scope: '', revision: 0, phase: 'unavailable' };
  private readonly listeners = new Set<() => void>();
  private readonly port: Port;
  constructor(port: Port) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(phase: State['phase']) {
    this.state = { scope: accountScopeKey(this.owner), revision: this.generation, phase };
    for (const listener of this.listeners) listener();
  }
  private current(generation = this.generation) {
    return Boolean(
      this.owner &&
      generation === this.generation &&
      accountScopeKey(this.owner) === accountScopeKey(this.port.scope()),
    );
  }
  activate(scope: AccountScope | null) {
    this.retire();
    this.owner = scope;
    if (!scope) return;
    this.publish('opening');
    if (!this.observe()) void this.retry(this.generation);
  }
  retire() {
    ++this.generation;
    this.owner = null;
    this.pending = false;
    this.opened = false;
    this.publish('unavailable');
  }
  observe(): boolean {
    if (!this.current() || this.opened) return this.opened;
    const summary = this.port.summary();
    if (summary?.kind !== 'saved' || !summary.can_write) return false;
    this.opened = true;
    try {
      this.port.open(summary.id);
      return true;
    } catch {
      this.opened = false;
      this.publish('failed');
      return false;
    }
  }
  async retry(revision: number) {
    if (!this.current(revision) || this.pending || this.observe()) return;
    const owner = this.owner!;
    const generation = this.generation;
    this.pending = true;
    this.publish('opening');
    try {
      await this.port.ensure(owner);
      if (this.current(generation) && !this.observe()) this.publish('waiting');
    } catch {
      if (this.current(generation) && !this.observe()) this.publish('failed');
    } finally {
      if (this.current(generation)) this.pending = false;
    }
  }
}
