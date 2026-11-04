import { localMessageDay } from './conversation-date.ts';
import type { SearchPeriod } from './generated/SearchPeriod';
export function searchDateRange(start: Date, end: Date): SearchPeriod | null {
  try {
    const first = localMessageDay(start),
      last = localMessageDay(end);
    const start_ms = Number(first.start_ms),
      end_ms = Number(last.end_ms);
    return Number.isSafeInteger(start_ms) &&
      Number.isSafeInteger(end_ms) &&
      start_ms >= 0 &&
      start_ms < end_ms &&
      end_ms <= 8_640_000_000_000_000
      ? { start_ms, end_ms }
      : null;
  } catch {
    return null;
  }
}
export type SearchDateRangeState = Readonly<{
  presented: boolean;
  token: number;
  start: number;
  end: number;
}>;
/** A disposable calendar proposal. The search controller owns the applied filter. */
export class SearchDateRange {
  private state: SearchDateRangeState = { presented: false, token: 0, start: 0, end: 0 };
  private focused = false;
  private listeners = new Set<() => void>();
  private ports: { current(): boolean; apply(value: SearchPeriod): void } | null;
  constructor(ports: SearchDateRange['ports'] = null) {
    this.ports = ports;
  }
  bind(ports: NonNullable<SearchDateRange['ports']>) {
    this.ports = ports;
    return () => {
      if (this.ports === ports) this.ports = null;
    };
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private update(change: Partial<SearchDateRangeState>) {
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
  open(value?: SearchPeriod, now = Date.now()) {
    if (!this.focused || this.ports?.current() !== true) return false;
    this.update({
      presented: true,
      token: this.state.token + 1,
      start: value?.start_ms ?? now,
      end: value ? value.end_ms - 1 : now,
    });
    return true;
  }
  close(token?: number) {
    if (token !== undefined && token !== this.state.token) return;
    this.update({ presented: false, token: this.state.token + 1 });
  }
  pick(token: number, field: 'start' | 'end', value: Date) {
    if (
      !this.focused ||
      !this.state.presented ||
      token !== this.state.token ||
      this.ports?.current() !== true ||
      !Number.isFinite(value.getTime())
    )
      return;
    this.update({ [field]: value.getTime() });
  }
  apply(observed: SearchDateRangeState) {
    if (
      observed !== this.state ||
      !observed.presented ||
      !this.focused ||
      this.ports?.current() !== true
    )
      return false;
    const value = searchDateRange(new Date(observed.start), new Date(observed.end));
    if (!value) return false;
    this.close(observed.token);
    this.ports?.apply(value);
    return true;
  }
}
