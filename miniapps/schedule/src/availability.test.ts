import assert from 'node:assert/strict';
import test from 'node:test';
import { availabilityScope, conflicts, createAvailability } from './availability.ts';
import { freeSlots } from './slots.ts';

const day = { y: 2026, m: 10, d: 19 };
const scope = availabilityScope(day, 'America/Sao_Paulo', 'alice');
const deferred = () => {
  let resolve!: (value: unknown) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<unknown>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

test('selected calendar day honors 23-hour and 25-hour DST boundaries', () => {
  const spring = availabilityScope({ y: 2026, m: 3, d: 8 }, 'America/New_York', 'alice');
  const fall = availabilityScope({ y: 2026, m: 11, d: 1 }, 'America/New_York', 'alice');
  assert.equal(spring.endMs - spring.startMs, 23 * 3_600_000);
  assert.equal(fall.endMs - fall.startMs, 25 * 3_600_000);
  assert.ok(spring.endMs - spring.startMs < 31 * 86_400_000);
  assert.throws(() => availabilityScope({ y: 2026, m: 2, d: 31 }, 'UTC', 'alice'));
});

test('only an explicit check reads the device; valid empty and denial are distinct', async () => {
  let calls = 0;
  const calendar = createAvailability(async (start, end) => {
    calls++;
    assert.deepEqual([start, end], [scope.startMs, scope.endMs]);
    if (calls === 2) throw new Error('Permission denied');
    return [];
  });
  calendar.setScope(scope);
  assert.equal(calls, 0);
  await calendar.check(scope);
  assert.deepEqual(calendar.snapshot(), { scope: scope.key, phase: 'ready', busy: [] });
  await calendar.check(scope);
  assert.deepEqual(calendar.snapshot(), {
    scope: scope.key,
    phase: 'error',
    message: 'Permission denied',
  });
  calendar.clear(scope);
  assert.equal(calls, 2);
  assert.equal(calendar.snapshot().phase, 'idle');
});

test('day, timezone and viewer changes retire late results, including ABA', async () => {
  for (const next of [
    availabilityScope({ ...day, d: 20 }, scope.timeZone, 'alice'),
    availabilityScope(day, 'Europe/London', 'alice'),
    availabilityScope(day, scope.timeZone, 'bob'),
  ]) {
    const first = deferred();
    const second = deferred();
    let n = 0;
    const calendar = createAvailability(() => (++n === 1 ? first.promise : second.promise));
    calendar.setScope(scope);
    const old = calendar.check(scope);
    calendar.setScope(next);
    calendar.setScope(scope);
    const fresh = calendar.check(scope);
    second.resolve([]);
    await fresh;
    first.resolve([{ startMs: scope.startMs, endMs: scope.endMs }]);
    await old;
    assert.deepEqual(calendar.snapshot(), { scope: scope.key, phase: 'ready', busy: [] });
  }
});

test('clear suppresses an old same-day response and disposal erases data and late errors', async () => {
  const old = deferred();
  const current = deferred();
  let calls = 0;
  const calendar = createAvailability(() => (++calls === 1 ? old.promise : current.promise));
  calendar.setScope(scope);
  const first = calendar.check(scope);
  calendar.clear(scope);
  const second = calendar.check(scope);
  old.resolve([{ startMs: scope.startMs, endMs: scope.endMs }]);
  await first;
  assert.equal(calendar.snapshot().phase, 'loading');
  calendar.dispose();
  current.reject(new Error('late permission denial'));
  await second;
  assert.deepEqual(calendar.snapshot(), { scope: '', phase: 'idle' });
  assert.equal(calendar.canBook(scope, { startMs: scope.startMs, endMs: scope.endMs }), false);
});

test('only bounded timestamps survive, with no mutation of business availability', async () => {
  const input = [
    {
      startMs: scope.startMs - 1000,
      endMs: scope.startMs + 1000,
      title: 'Private',
      location: 'Private',
    },
  ];
  const calendar = createAvailability(async () => input);
  calendar.setScope(scope);
  await calendar.check(scope);
  assert.deepEqual(calendar.snapshot(), {
    scope: scope.key,
    phase: 'ready',
    busy: [{ startMs: scope.startMs, endMs: scope.startMs + 1000 }],
  });
  assert.equal(input[0].startMs, scope.startMs - 1000);
  const state = {
    weeklyHours: { '1': [['09:00', '10:00']] as [string, string][] },
    slotMinutes: 30,
    timezone: scope.timeZone,
    horizonDays: 60,
    busy: [],
  };
  const before = structuredClone(state);
  const slots = freeSlots({
    day,
    service: { id: 's', name: 'Session', minutes: 30, priceCents: 0 },
    state,
    nowMs: scope.startMs,
  });
  assert.equal(slots.length, 2);
  assert.deepEqual(state, before);
});

test('malformed intervals fail closed rather than imply a free calendar', async () => {
  for (const value of [
    null,
    {},
    [{ startMs: 1, endMs: 0 }],
    [{ startMs: '1', endMs: 2 }],
    [{ startMs: 1, endMs: Infinity }],
  ]) {
    const calendar = createAvailability(async () => value);
    calendar.setScope(scope);
    await calendar.check(scope);
    assert.equal(calendar.snapshot().phase, 'error');
  }
});

test('live booking guard rejects newly conflicting selections and allows boundary-touching slots', async () => {
  const pending = deferred();
  const calendar = createAvailability(() => pending.promise);
  calendar.setScope(scope);
  const slot = { startMs: scope.startMs + 1000, endMs: scope.startMs + 2000 };
  assert.equal(calendar.canBook(scope, slot), true);
  const checking = calendar.check(scope);
  assert.equal(calendar.canBook(scope, slot), false);
  pending.resolve([slot]);
  await checking;
  assert.equal(calendar.canBook(scope, slot), false);
  assert.equal(conflicts({ startMs: slot.endMs, endMs: slot.endMs + 1000 }, [slot]), false);
  assert.equal(conflicts({ startMs: slot.startMs - 1000, endMs: slot.startMs }, [slot]), false);
  calendar.clear(scope);
  assert.equal(calendar.canBook(scope, slot), true);
});
