import { addDays, dayKey, sameDay, todayIn, zonedMs, type Ymd } from './slots.ts';

export type Interval = { startMs: number; endMs: number };
export type AvailabilityScope = Interval & { key: string; day: string; timeZone: string };
export type Availability =
  | { scope: string; phase: 'idle' | 'loading' }
  | { scope: string; phase: 'ready'; busy: Interval[] }
  | { scope: string; phase: 'error'; message: string };

/** Calendar midnights, rather than a fixed 24 hours, preserve DST day boundaries. */
export function availabilityScope(day: Ymd, timeZone: string, viewerId: string): AvailabilityScope {
  const startMs = zonedMs(day, 0, timeZone);
  const endMs = zonedMs(addDays(day, 1), 0, timeZone);
  if (
    !sameDay(todayIn(timeZone, startMs), day) ||
    !Number.isSafeInteger(startMs) ||
    !Number.isSafeInteger(endMs) ||
    endMs <= startMs ||
    endMs - startMs > 31 * 86_400_000
  ) {
    throw new Error('Invalid calendar day');
  }
  return {
    key: JSON.stringify([viewerId, dayKey(day), timeZone]),
    day: dayKey(day),
    timeZone,
    startMs,
    endMs,
  };
}

function busyIntervals(value: unknown, scope: AvailabilityScope): Interval[] {
  if (!Array.isArray(value) || value.length > 10_000) throw new Error('Invalid calendar response');
  return value
    .map((block: unknown) => {
      if (!block || typeof block !== 'object') throw new Error('Invalid calendar response');
      const { startMs, endMs } = block as Interval;
      if (!Number.isSafeInteger(startMs) || !Number.isSafeInteger(endMs) || endMs <= startMs) {
        throw new Error('Invalid calendar response');
      }
      // Copy only intervals; never retain any event fields returned by an unexpected host.
      return { startMs: Math.max(startMs, scope.startMs), endMs: Math.min(endMs, scope.endMs) };
    })
    .filter((block) => block.endMs > block.startMs);
}

export const conflicts = (slot: Interval, busy: readonly Interval[]) =>
  busy.some((block) => block.startMs < slot.endMs && block.endMs > slot.startMs);

/** Private, disposable view state. No storage or shared Schedule mutation belongs here. */
export function createAvailability(read: (startMs: number, endMs: number) => Promise<unknown>) {
  let generation = 0;
  let live = true;
  let state: Availability = { scope: '', phase: 'idle' };
  const listeners = new Set<() => void>();
  const publish = (next: Availability) => {
    state = next;
    for (const listener of listeners) listener();
  };
  const current = (scope: AvailabilityScope) => state.scope === scope.key;
  return {
    snapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    setScope(scope: AvailabilityScope) {
      if (!live || current(scope)) return;
      generation++;
      publish({ scope: scope.key, phase: 'idle' });
    },
    clear(scope: AvailabilityScope) {
      if (!live || !current(scope)) return;
      generation++;
      publish({ scope: scope.key, phase: 'idle' });
    },
    async check(scope: AvailabilityScope) {
      if (!live || !current(scope) || state.phase === 'loading') return;
      const request = ++generation;
      publish({ scope: scope.key, phase: 'loading' });
      try {
        const value = await read(scope.startMs, scope.endMs);
        if (!live || generation !== request || !current(scope)) return;
        publish({ scope: scope.key, phase: 'ready', busy: busyIntervals(value, scope) });
      } catch (error) {
        if (!live || generation !== request || !current(scope)) return;
        publish({
          scope: scope.key,
          phase: 'error',
          message: error instanceof Error ? error.message : '',
        });
      }
    },
    // Check the live snapshot at submission time, including a response arriving after render.
    canBook(scope: AvailabilityScope, slot: Interval) {
      return (
        live &&
        current(scope) &&
        state.phase !== 'loading' &&
        (state.phase !== 'ready' || !conflicts(slot, state.busy))
      );
    },
    dispose() {
      live = false;
      generation++;
      state = { scope: '', phase: 'idle' };
      listeners.clear();
    },
  };
}
