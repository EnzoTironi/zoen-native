import type { MediaDraftScope } from './generated/MediaDraftScope';

/** Calendar arithmetic, not a fixed 24-hour duration: DST belongs to the reader's day. */
export function localMessageDay(date: Date): { start_ms: string; end_ms: string } {
  if (!Number.isFinite(date.getTime())) throw new Error('Invalid calendar day');
  const start = new Date(date.getTime());
  start.setHours(0, 0, 0, 0);
  const end = new Date(start.getTime());
  end.setDate(end.getDate() + 1);
  if (!Number.isFinite(end.getTime()) || start.getTime() < 0 || end.getTime() <= start.getTime())
    throw new Error('Invalid calendar day');
  return { start_ms: String(start.getTime()), end_ms: String(end.getTime()) };
}

export type CalendarJumpState = Readonly<{
  presented: boolean;
  presentation: number;
  date: number;
  pending: boolean;
  error: 'missing' | 'failed' | null;
}>;
type Ports = {
  current(): boolean;
  read(scope: MediaDraftScope, day: ReturnType<typeof localMessageDay>): Promise<string | null>;
  reveal(id: string): Promise<unknown>;
  select(id: string): void;
};

/** Owns transient navigation only; Core remains the authority for visibility and history. */
export class ConversationDateJump {
  private state: CalendarJumpState;
  private generation = 0;
  private focused = false;
  private listeners = new Set<() => void>();
  private readonly scope: MediaDraftScope;
  private readonly ports: Ports;
  constructor(scope: MediaDraftScope, ports: Ports, now = Date.now()) {
    this.scope = Object.freeze({ ...scope });
    this.ports = ports;
    this.state = { presented: false, presentation: 0, date: now, pending: false, error: null };
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(change: Partial<CalendarJumpState>) {
    this.state = { ...this.state, ...change };
    for (const listener of this.listeners) listener();
  }
  focus() {
    this.focused = true;
  }
  blur() {
    this.focused = false;
    this.close();
  }
  private current() {
    return this.focused && this.ports.current();
  }
  open() {
    if (!this.current() || this.state.presented) return false;
    this.publish({ presented: true, presentation: ++this.generation, error: null });
    return true;
  }
  close(presentation?: number) {
    if (presentation !== undefined && presentation !== this.state.presentation) return;
    ++this.generation;
    this.publish({ presented: false, pending: false, error: null });
  }
  setDate(date: Date, presentation = this.state.presentation) {
    if (presentation !== this.state.presentation) return;
    if (!this.state.presented || this.state.pending || !this.current()) return;
    try {
      localMessageDay(date);
    } catch {
      this.publish({ error: 'failed' });
      return;
    }
    ++this.generation;
    this.publish({ date: date.getTime(), error: null });
  }
  async jump(observed = this.state) {
    if (observed !== this.state) return;
    if (!this.state.presented || this.state.pending || !this.current()) return;
    const generation = ++this.generation;
    const current = () => generation === this.generation && this.current();
    this.publish({ pending: true, error: null });
    try {
      if (!current()) return;
      const target = await this.ports.read(this.scope, localMessageDay(new Date(this.state.date)));
      if (!current()) return;
      if (!target) {
        this.publish({ pending: false, error: 'missing' });
        return;
      }
      await this.ports.reveal(target);
      if (!current()) return;
      this.close();
      this.ports.select(target);
    } catch {
      if (current()) this.publish({ pending: false, error: 'failed' });
    }
  }
}
