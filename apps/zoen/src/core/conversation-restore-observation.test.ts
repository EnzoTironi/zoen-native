import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  ConversationRestoreObservation,
  type RestoreObservationContext,
} from './conversation-restore-observation.ts';
import type { ConversationNativeMetrics } from './conversation-reveal.ts';
import { deferred } from './store.fixtures.ts';

const packet = (sequence = '1'): ConversationNativeMetrics => ({
  binding: 'owned',
  sequence,
  geometry: {
    contentOffset: { y: 120 },
    contentSize: { height: 900 },
    layoutMeasurement: { height: 600 },
    contentInset: { top: 80, bottom: 240 },
  },
});
function fixture() {
  const records: Readonly<Record<string, unknown>>[] = [];
  const context: {
    -readonly [K in keyof RestoreObservationContext]: RestoreObservationContext[K];
  } = {
    searchActive: false,
    reply: true,
    editing: false,
    keyboardHeight: 0,
    keyboardProgress: 0,
    list: {
      firstItemOffset: 0,
      childHeight: 900,
      windowHeight: 600,
      lastRowY: 850,
      lastRowHeight: 50,
    },
  };
  let current = true;
  const probe = new ConversationRestoreObservation('owned', 0);
  probe.connect({
    current: () => current,
    context: () => context,
    emit: (r) => records.push(r),
  });
  return {
    records,
    context,
    probe,
    loseOwner: () => {
      current = false;
    },
  };
}
test('unchanged native geometry is sampled before/during/after search without private context', async () => {
  const f = fixture(),
    native = { ...packet(), query: 'secret query', owner: 'private owner' };
  await f.probe.sample(async () => native);
  f.context.searchActive = true;
  await f.probe.sample(async () => ({ ...native, sequence: '2' }));
  f.context.searchActive = false;
  f.context.keyboardHeight = 279.333;
  await f.probe.sample(async () => ({ ...native, sequence: '3' }));
  assert.deepEqual(
    f.records.map((r) => r.phase),
    ['before-search', 'search', 'restored'],
  );
  assert.deepEqual(f.records[2]!.native, packet().geometry);
  for (const privateValue of ['secret', 'private', 'owned'])
    assert.ok(!JSON.stringify(f.records).includes(privateValue));
});
for (const edge of ['retire', 'owner-loss'] as const)
  test(`pending native sample cannot log after ${edge}`, async () => {
    const f = fixture(),
      pending = deferred<ConversationNativeMetrics | null>();
    const read = f.probe.sample(() => pending.promise);
    if (edge === 'retire') f.probe.retire();
    else f.loseOwner();
    pending.resolve(packet());
    await read;
    f.probe.receive(packet('2'));
    assert.deepEqual(f.records, []);
  });
test('foreign, malformed, duplicate and out-of-order observations do not become measurements', () => {
  const f = fixture();
  f.probe.receive({ ...packet(), binding: 'other' });
  f.probe.receive({ ...packet(), sequence: 'invalid' });
  f.probe.receive({
    ...packet(),
    geometry: { ...packet().geometry, contentInset: { bottom: NaN } },
  });
  assert.deepEqual(f.records, []);
  f.probe.receive(packet('5'));
  f.probe.receive(packet('4'));
  f.probe.receive(packet('5'));
  f.probe.receive(packet('6'));
  assert.equal(f.records.length, 1);
});
test('bounded per-phase records let restoration remain visible after a long initial scroll', () => {
  const f = fixture();
  for (let n = 1; n <= 100; n++) {
    const p = packet(String(n));
    f.probe.receive({ ...p, geometry: { ...p.geometry, contentOffset: { y: n } } });
  }
  assert.equal(f.records.length, 32);
  f.context.searchActive = true;
  f.probe.receive(packet('101'));
  f.context.searchActive = false;
  f.probe.receive(packet('102'));
  assert.deepEqual(
    f.records.slice(-2).map((r) => r.phase),
    ['search', 'restored'],
  );
});

test('unavailable or throwing QA ports cannot affect native event delivery', async () => {
  const probe = new ConversationRestoreObservation('owned', 0);
  probe.connect({
    current: () => true,
    context: () => {
      throw Error('layout retired');
    },
    emit: () => assert.fail('unavailable layout cannot emit'),
  });
  assert.doesNotThrow(() => probe.receive(packet()));
  await probe.sample(async () => {
    throw Error('native view retired');
  });
  assert.doesNotThrow(() => probe.composerLayout(279));
  assert.doesNotThrow(() => probe.insetRequested({ top: 0, bottom: 279 }));
  await probe.sample(async () => assert.fail('an unavailable context must not read UIKit'));
});

test('restoration records measured composer and requested inset separately from actual UIKit facts', () => {
  const f = fixture();
  assert.equal(f.probe.composerLayout(279.333), true);
  assert.equal(f.probe.insetRequested({ top: 0, bottom: 279.333 }), true);
  f.probe.receive(packet('1'));
  f.context.searchActive = true;
  f.probe.composerLayout(91.333);
  f.probe.insetRequested({ top: 0, bottom: 393.333 });
  f.probe.receive(packet('2'));
  f.context.searchActive = false;
  f.probe.composerLayout(279.333);
  f.probe.insetRequested({ top: 0, bottom: 581.333 });
  f.probe.receive(packet('3'));
  assert.deepEqual(
    f.records.map((r) => r.phase),
    ['before-search', 'search', 'restored'],
  );
  assert.deepEqual(f.records[2]!.composer, { height: 279.333 });
  assert.deepEqual(f.records[2]!.requestedInset, { top: 0, bottom: 581.333 });
  assert.deepEqual(f.records[2]!.native, packet().geometry, 'desired values cannot replace UIKit');
  assert.deepEqual(f.records[2]!.list, f.context.list);
});

test('invalid or repeated desired measurements do not trigger a new read or poll', () => {
  const f = fixture();
  for (const value of [0, -1, NaN, Infinity]) assert.equal(f.probe.composerLayout(value), false);
  for (const value of [-1, NaN, Infinity]) {
    assert.equal(f.probe.insetRequested({ top: value, bottom: 240 }), false);
    assert.equal(f.probe.insetRequested({ top: 0, bottom: value }), false);
  }
  assert.equal(f.probe.composerLayout(279), true);
  assert.equal(f.probe.composerLayout(279), false);
  assert.equal(f.probe.insetRequested({ top: 0, bottom: 0 }), true, 'SDK initialization is valid');
  assert.equal(f.probe.insetRequested({ top: 0, bottom: 0 }), false);
  f.probe.receive(packet());
  assert.deepEqual(f.records[0]!.composer, { height: 279 });
});

for (const change of ['composer', 'requested-inset', 'phase'] as const)
  test(`an awaited sample cannot label older native geometry with a newer ${change}`, async () => {
    const f = fixture();
    const pending = deferred<ConversationNativeMetrics | null>();
    const read = f.probe.sample(() => pending.promise);
    if (change === 'composer') f.probe.composerLayout(279);
    else if (change === 'requested-inset') f.probe.insetRequested({ top: 0, bottom: 279 });
    else f.context.searchActive = true;
    pending.resolve(packet());
    await read;
    assert.deepEqual(f.records, []);
    f.probe.receive(packet('2'));
    assert.equal(f.records.length, 1, 'a real later notification remains observable');
  });

for (const change of ['retire', 'owner-loss'] as const)
  test(`late measurement callbacks after ${change} do not change observation context`, () => {
    const f = fixture();
    if (change === 'retire') f.probe.retire();
    else f.loseOwner();
    assert.equal(f.probe.composerLayout(279), false);
    assert.equal(f.probe.insetRequested({ top: 0, bottom: 279 }), false);
    f.probe.receive(packet());
    assert.deepEqual(f.records, []);
  });

test('a later phase sample fences an older read even after returning to the original phase', async () => {
  const f = fixture();
  const before = deferred<ConversationNativeMetrics | null>();
  const search = deferred<ConversationNativeMetrics | null>();
  const restored = deferred<ConversationNativeMetrics | null>();
  const first = f.probe.sample(() => before.promise);
  f.context.searchActive = true;
  const second = f.probe.sample(() => search.promise);
  f.context.searchActive = false;
  const third = f.probe.sample(() => restored.promise);
  before.resolve(packet('1'));
  search.resolve(packet('2'));
  await Promise.all([first, second]);
  assert.equal(f.records.length, 0);
  restored.resolve(packet('3'));
  await third;
  assert.equal(f.records.length, 1);
  assert.equal(f.records[0]!.phase, 'restored');
});
