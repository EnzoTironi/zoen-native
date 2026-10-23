import { integer, object } from './boundary.ts';

export type CursorRepairStatus = { admission: number | null; deadline: number };
type Config = { version: 1; interval: number; timeout: number; initial: number };
const MAX_TIMER = 2_147_483_647;
const MAX_INPUT_CLOCK = 281_474_976_710_655;

function clock(value: unknown): number {
  const result = integer(value);
  if (result < 0) throw new Error('Invalid reconciliation clock.');
  return result;
}
function duration(value: unknown): number {
  const result = integer(value);
  if (result < 1 || result > MAX_TIMER) throw new Error('Invalid reconciliation policy.');
  return result;
}
export function parseCursorRepairStatus(value: unknown): CursorRepairStatus | null {
  if (value === undefined || value === null) return null;
  const input = object(value);
  const admission = input.admission_ms == null ? null : clock(input.admission_ms);
  const deadline = clock(input.abort_deadline_ms);
  if (admission !== null && admission > deadline)
    throw new Error('Invalid reconciliation deadline.');
  return { admission, deadline };
}

/** Only socket-local timers live here. Rust owns ordered Sync bookkeeping, and
 * verified encrypted snapshots remain under the existing persistence authority. */
export class CursorReconciliation {
  private config: Config | undefined;
  private generation: number | undefined;
  private status: CursorRepairStatus | undefined;
  private admission: ReturnType<typeof setTimeout> | undefined;
  private watchdog: ReturnType<typeof setTimeout> | undefined;
  private readonly allowed: (generation: number) => boolean;
  private readonly tick: (now: number, generation: number) => Promise<void>;
  private readonly reconnect: () => void;

  constructor(ports: {
    allowed: (generation: number) => boolean;
    tick: (now: number, generation: number) => Promise<void>;
    reconnect: () => void;
  }) {
    this.allowed = ports.allowed;
    this.tick = ports.tick;
    this.reconnect = ports.reconnect;
  }

  now(): number {
    const now = clock(Math.floor(performance.now()));
    if (now > MAX_INPUT_CLOCK) throw new Error('Invalid reconciliation clock.');
    return now;
  }

  configure(json: string): void {
    if (this.generation !== undefined) throw new Error('Reconciliation is already active.');
    const input = object(JSON.parse(json));
    if (input.version !== 1) throw new Error('Unsupported reconciliation policy.');
    this.config = {
      version: 1,
      interval: duration(input.interval_ms),
      timeout: duration(input.timeout_ms),
      initial: duration(input.initial_inactivity_ms),
    };
  }

  start(generation: number, now: number): void {
    if (!this.config) throw new Error('The encrypted core has no reconciliation policy.');
    if (this.generation !== undefined)
      throw new Error('Reconciliation already has a socket owner.');
    if (clock(now) > MAX_INPUT_CLOCK) throw new Error('Invalid reconciliation clock.');
    this.generation = generation;
    this.update({ admission: null, deadline: clock(now + this.config.initial) }, generation);
  }

  update(status: CursorRepairStatus | null, generation: number): void {
    if (this.generation !== generation || !this.allowed(generation)) return;
    // A durable save may finish after the previous budget even if its timer
    // callback has not run yet. Late metadata cannot revive that socket owner.
    if (this.status && this.now() >= this.status.deadline) {
      this.expire(generation);
      return;
    }
    // An omitted notification cannot clear or renew a previously established
    // budget. Old/malformed Core configuration is refused before transport.start.
    if (!status) return;
    this.status = status;
    clearTimeout(this.admission);
    clearTimeout(this.watchdog);
    if (this.now() >= status.deadline) {
      this.expire(generation);
      return;
    }
    const current = status;
    this.watchdog = setTimeout(() => {
      if (this.status !== current || !this.current(generation)) return;
      if (this.now() >= current.deadline) this.expire(generation);
      else this.update(current, generation);
    }, this.delay(status.deadline));
    if (status.admission !== null) {
      this.admission = setTimeout(() => {
        if (this.status !== current || !this.current(generation)) return;
        // A suspended tab may resume after both timers are due. Do not admit a
        // late full Sync merely because this callback wins the event-loop race.
        if (this.now() >= current.deadline) {
          this.expire(generation);
          return;
        }
        void this.tick(this.now(), generation).catch(() => {});
      }, this.delay(status.admission));
    }
  }

  current(generation: number): boolean {
    return this.generation === generation && this.allowed(generation);
  }

  check(generation: number): boolean {
    if (!this.current(generation) || !this.status) return false;
    if (this.now() >= this.status.deadline) {
      this.expire(generation);
      return false;
    }
    return true;
  }

  expire(generation: number): void {
    if (!this.current(generation)) return;
    this.retire();
    this.reconnect();
  }

  retire(): void {
    this.generation = undefined;
    this.status = undefined;
    clearTimeout(this.admission);
    clearTimeout(this.watchdog);
    this.admission = undefined;
    this.watchdog = undefined;
  }

  private delay(until: number): number {
    const remaining = Math.max(0, until - this.now());
    if (remaining > MAX_TIMER) throw new Error('Reconciliation exceeds the browser timer range.');
    return remaining;
  }
}
