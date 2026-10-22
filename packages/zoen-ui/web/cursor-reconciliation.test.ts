import test from 'node:test';
import assert from 'node:assert/strict';
import { CursorReconciliation, parseCursorRepairStatus } from './cursor-reconciliation.ts';

const policy = JSON.stringify({
  version: 1,
  interval_ms: 60_000,
  timeout_ms: 30_000,
  initial_inactivity_ms: 60_000,
});
function fixture(t: test.TestContext) {
  let now = 0,
    active = true,
    reconnects = 0;
  const ticks: Array<[number, number]> = [];
  t.mock.timers.enable({ apis: ['setTimeout'] });
  t.mock.method(performance, 'now', () => now);
  const repair = new CursorReconciliation({
    allowed: (generation) => active && generation === 1,
    tick: async (clock, generation) => {
      ticks.push([clock, generation]);
    },
    reconnect: () => {
      ++reconnects;
      active = false;
    },
  });
  repair.configure(policy);
  t.after(() => repair.retire());
  return {
    repair,
    ticks,
    reconnects: () => reconnects,
    set: (value: number) => {
      now = value;
    },
    advance: (value: number) => {
      now += value;
      t.mock.timers.tick(value);
    },
  };
}

test('cursor reconciliation admits once at the absolute Core deadline and Pong cannot renew it', (t) => {
  const f = fixture(t);
  f.repair.start(1, 0);
  f.repair.update({ admission: 60_000, deadline: 90_000 }, 1);
  f.advance(30_000);
  f.repair.update(null, 1); // A Pong has no durable cursor progress.
  f.advance(30_000);
  assert.deepEqual(f.ticks, [[60_000, 1]]);
  assert.equal(f.reconnects(), 0);
  f.repair.update({ admission: null, deadline: 90_000 }, 1);
  f.advance(29_999);
  assert.equal(f.repair.check(1), true);
  f.advance(1);
  assert.equal(f.reconnects(), 1);
  assert.equal(f.repair.check(1), false);
  f.advance(90_000);
  assert.equal(f.reconnects(), 1);
  assert.equal(f.ticks.length, 1);
});

test('a suspended tab retires before admitting full Sync when both timers are overdue', (t) => {
  const f = fixture(t);
  f.repair.start(1, 0);
  f.repair.update({ admission: 60_000, deadline: 90_000 }, 1);
  f.advance(90_000);
  assert.deepEqual(f.ticks, []);
  assert.equal(f.reconnects(), 1);
});

test('metadata completing after the previous budget cannot revive the socket even before timer delivery', (t) => {
  const f = fixture(t);
  f.repair.start(1, 0);
  f.set(60_000); // Promise completion wins before the already-ready watchdog callback.
  f.repair.update({ admission: 120_000, deadline: 150_000 }, 1);
  assert.equal(f.reconnects(), 1);
  assert.equal(f.repair.current(1), false);
  f.advance(1);
  assert.deepEqual(f.ticks, []);
});

test('persisted progress keeps its ingestion deadline instead of gaining time from a slow save', (t) => {
  const f = fixture(t);
  f.repair.start(1, 0);
  f.set(40_000);
  f.repair.update({ admission: null, deadline: 61_000 }, 1); // Progress ingested at 1s.
  f.advance(20_999);
  assert.equal(f.reconnects(), 0);
  f.advance(1);
  assert.equal(f.reconnects(), 1);
});

test('retirement and stale metadata cannot restart timers for a closed generation', (t) => {
  const f = fixture(t);
  f.repair.start(1, 0);
  f.repair.update({ admission: 60_000, deadline: 90_000 }, 1);
  f.repair.retire();
  f.repair.update({ admission: 1, deadline: 2 }, 1);
  f.advance(100_000);
  assert.equal(f.reconnects(), 0);
  assert.deepEqual(f.ticks, []);
});

test('unsupported policy and invalid clock/metadata fail closed without duplicate ownership', (t) => {
  const f = fixture(t);
  for (const value of [
    undefined,
    {},
    { version: 0 },
    { version: 1, interval_ms: 0, timeout_ms: 30_000, initial_inactivity_ms: 60_000 },
    { version: 1, interval_ms: 60_000, timeout_ms: 2 ** 31, initial_inactivity_ms: 60_000 },
  ]) {
    const fresh = new CursorReconciliation({
      allowed: () => true,
      tick: async () => {},
      reconnect: () => assert.fail('No owner was admitted'),
    });
    assert.throws(() => fresh.configure(JSON.stringify(value)));
    assert.throws(() => fresh.start(1, 0), /no reconciliation policy/);
  }
  for (const value of [-1, 0.5, Number.NaN, 281_474_976_710_656])
    assert.throws(() => f.repair.start(1, value));
  f.repair.start(1, 0);
  assert.throws(() => f.repair.start(1, 0), /socket owner/);
  for (const value of [
    { admission_ms: 2, abort_deadline_ms: 1 },
    { abort_deadline_ms: null },
    { abort_deadline_ms: Number.MAX_SAFE_INTEGER + 1 },
    { abort_deadline_ms: -1 },
  ])
    assert.throws(() => parseCursorRepairStatus(value));
  assert.equal(parseCursorRepairStatus(undefined), null);
  f.set(281_474_976_710_656);
  assert.throws(() => f.repair.now(), /clock/);
});
