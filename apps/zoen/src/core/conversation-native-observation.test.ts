import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, geometry, item } from './conversation-reveal.fixtures.ts';
import type { ConversationNativeMetrics } from './conversation-reveal.ts';

const tick = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
function setup() {
  const records: Readonly<Record<string, unknown>>[] = [];
  const reads: ((value: ConversationNativeMetrics | null) => void)[] = [];
  const f = fixture(
    (record) => records.push(record),
    () => new Promise((done) => reads.push(done)),
  );
  const native = geometry({
    contentOffset: { y: 0 },
    contentSize: { height: 1783 },
    layoutMeasurement: { height: 402 },
    contentInset: { top: 0, bottom: 275.33333587646484 },
  });
  f.ready({ ...native, contentOffset: { y: 223.66666666666666 } });
  f.reveal.onInset({ top: 0, bottom: 289.33333587646484 });
  f.setRow(item({ y: 0, height: 600, firstItemOffset: 64, childHeight: 1719, windowHeight: 402 }));
  const packet = (
    sequence: string,
    patch: Partial<ConversationNativeMetrics> = {},
  ): ConversationNativeMetrics => ({
    binding: f.reveal.nativeBinding,
    sequence,
    geometry: native,
    ...patch,
  });
  return { f, records, reads, native, packet };
}

test('an explicit current UIKit sample completes no-op/inset-only search without another scroll event or user gesture', async () => {
  const { f, records, reads, packet } = setup();
  f.reveal.request('hit');
  f.frame();
  assert.equal(reads.length, 1);
  assert.deepEqual(f.offsets, []);
  reads[0]!(packet('1'));
  await tick();
  f.frame();
  assert.deepEqual(f.offsets, [], 'current offset is already correct; do not force a scroll');
  assert.equal(records.filter((r) => r.event === 'retire').length, 1);
  assert.equal(records.find((r) => r.event === 'retire')!.nativeObserved, true);
  assert.equal(reads.length, 1, 'completion does not poll');
});

for (const retire of ['query', 'owner', 'drag'] as const)
  test(`an awaited UIKit sample cannot complete a retired ${retire} scope`, async () => {
    const { f, records, reads, packet } = setup();
    f.reveal.request('old');
    f.frame();
    if (retire === 'owner') f.setCurrent(false);
    else f.reveal.cancel(retire);
    reads[0]!(packet('1'));
    await tick();
    if (retire === 'owner') f.frame();
    assert.deepEqual(f.offsets, []);
    assert.ok(!records.some((r) => r.event === 'retire'));
  });

test('new query after a delayed read requires its own native sample, never the old answer', async () => {
  const { f, records, reads, packet } = setup();
  f.reveal.request('old');
  f.frame();
  f.reveal.request('new');
  f.frame();
  assert.equal(reads.length, 1, 'at most one read is in flight');
  reads[0]!(packet('1'));
  await tick();
  f.frame();
  assert.equal(reads.length, 2);
  assert.ok(!records.some((r) => r.event === 'retire'));
  reads[1]!(packet('2'));
  await tick();
  f.frame();
  assert.equal(records.filter((r) => r.event === 'retire').length, 1);
});

for (const patch of [
  { binding: 'detached-view' },
  { sequence: '0' },
  { sequence: '18446744073709551616' },
  { sequence: '-1' },
  { sequence: '1.5' },
])
  test(`reject native binding/sequence ${JSON.stringify(patch)} without a read retry loop`, async () => {
    const { f, records, reads, packet } = setup();
    f.reveal.request('hit');
    f.frame();
    reads[0]!(packet('1', patch));
    await tick();
    assert.ok(!records.some((r) => r.event === 'retire'));
    assert.equal(reads.length, 1);
    assert.deepEqual(f.offsets, []);
  });

test('a newer native change invalidates an older in-flight response and coalesces duplicate signals', async () => {
  const { f, records, reads, packet } = setup();
  f.reveal.request('hit');
  f.frame();
  f.reveal.nativeMetricsChanged(packet('2'));
  f.reveal.nativeMetricsChanged(packet('2'));
  reads[0]!(packet('1'));
  await tick();
  f.frame();
  assert.equal(reads.length, 2);
  assert.ok(!records.some((r) => r.event === 'retire'));
  reads[1]!(packet('3'));
  await tick();
  f.frame();
  assert.equal(records.filter((r) => r.event === 'retire').length, 1);
  f.reveal.nativeMetricsChanged(packet('2'));
  assert.equal(reads.length, 2, 'stale callback cannot re-arm a completed target');
});

test('same query resumes after actual rotation/inset change and obtains fresh native facts', async () => {
  const { f, records, reads, packet, native } = setup();
  f.reveal.request('hit');
  f.frame();
  reads[0]!(packet('1'));
  await tick();
  f.frame();
  const changed = { ...native, contentInset: { top: 0, bottom: 300 } };
  f.reveal.nativeMetricsChanged(packet('2', { geometry: changed }));
  f.frame();
  assert.equal(reads.length, 2);
  reads[1]!(packet('3', { geometry: changed }));
  await tick();
  f.frame();
  assert.equal(records.filter((r) => r.event === 'retire').length, 2);
  assert.deepEqual(f.offsets, []);
});

test('a native observation predating the actual list commit cannot complete newly committed content', async () => {
  const { f, records, reads, packet } = setup();
  f.setData([{ id: 'replacement' }]);
  f.reveal.request('hit');
  f.frame();
  reads[0]!(packet('1'));
  await tick();
  assert.ok(!records.some((r) => r.event === 'retire'));
  f.commit();
  f.frame();
  assert.equal(reads.length, 2, 'the actual commit requires a new current native sample');
  reads[1]!(packet('2'));
  await tick();
  f.frame();
  assert.equal(records.filter((r) => r.event === 'retire').length, 1);
});

test('an unavailable native view stays pending without polling and recovers on a real bound-view signal', async () => {
  const { f, records, reads, packet } = setup();
  f.reveal.request('hit');
  f.frame();
  reads[0]!(null);
  await tick();
  assert.equal(reads.length, 1);
  assert.ok(!records.some((r) => r.event === 'retire'));
  f.reveal.nativeMetricsChanged(packet('1'));
  f.frame();
  reads[1]!(packet('2'));
  await tick();
  f.frame();
  assert.equal(records.filter((r) => r.event === 'retire').length, 1);
});
