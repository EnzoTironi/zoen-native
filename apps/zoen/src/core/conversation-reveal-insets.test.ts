import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, geometry, item } from './conversation-reveal.fixtures.ts';

const native470 = geometry({
  contentOffset: { y: 223.66666666666666 },
  contentSize: { height: 1783 },
  layoutMeasurement: { height: 402 },
  contentInset: { top: 0, bottom: 275.33333587646484 },
});
const expected470 = { top: 0, bottom: 289.33333587646484 };
const tallHit = item({
  y: 0,
  height: 600,
  firstItemOffset: 64,
  childHeight: 1719,
  windowHeight: 402,
  topObstruction: 64,
});

test('470 fresh landscape search can seek using measured geometry before an inset-only change emits a scroll', () => {
  const records: Readonly<Record<string, unknown>>[] = [];
  const f = fixture((record) => records.push(record));
  const entries = Array.from({ length: 11 }, (_, index) => ({ id: index }));
  f.setData(entries);
  f.commit();
  f.ready(native470);
  // SDK animatedProps and its effective-inset notification are independent of
  // native scroll events. This is the exact final 470 packet/inset pair.
  f.reveal.onInset(expected470);
  f.setRow(tallHit);
  f.reveal.request('landscape-hit');
  f.frame();
  assert.deepEqual(f.offsets, [0], 'the explicit search must not wait for its own scroll');
  assert.ok(!records.some((record) => record.event === 'retire'));
  const issued = records.find((record) => record.event === 'issue');
  assert.ok(issued);
  assert.equal(issued.ready, false, 'a desired inset is not a native acknowledgement');
  f.commit();
  f.reveal.onScroll({ ...native470, contentOffset: { y: 0 } });
  f.frame();
  assert.ok(!records.some((record) => record.event === 'retire'));
  assert.deepEqual(f.offsets, [0], 'unchanged geometry does not repeat the seek');
  f.reveal.onScroll({ ...native470, contentOffset: { y: 0 }, contentInset: expected470 });
  f.frame();
  assert.equal(records.filter((record) => record.event === 'retire').length, 1);
  assert.deepEqual(f.offsets, [0]);
});

test('the same search survives rotation but waits for the actual new viewport and committed data', () => {
  const f = fixture();
  f.ready();
  f.reveal.request('same-hit');
  const stale = f.frames.at(-1)!;
  f.reveal.onLayout(402);
  f.reveal.onInset(expected470);
  f.setData([{ id: 'rotated' }]);
  f.setRow(tallHit);
  stale.run();
  assert.deepEqual(f.offsets, []);
  f.reveal.onScroll(native470);
  assert.ok(f.frames.every((frame) => frame.cancelled));
  f.commit();
  f.frame();
  assert.deepEqual(f.offsets, [0]);
});

for (const nextBottom of [200, 500])
  test(`unacknowledged inset${nextBottom} does not replace native bounds or finish a clamped seek`, () => {
    const records: Readonly<Record<string, unknown>>[] = [];
    const f = fixture((record) => records.push(record));
    const native = geometry({ contentOffset: { y: 0 }, contentSize: { height: 2400 } });
    f.ready(native);
    f.setRow(item({ y: 2200, height: 136, childHeight: 2336 }));
    f.reveal.onInset({ top: 0, bottom: nextBottom });
    f.reveal.request('last-hit');
    f.frame();
    assert.deepEqual(f.offsets, [2000], 'use actual bottom360, never the desired future inset');
    f.commit();
    f.reveal.onScroll({ ...native, contentOffset: { y: 2000 } });
    f.frame();
    assert.ok(!records.some((record) => record.event === 'retire'));
    f.reveal.onScroll({
      ...native,
      contentOffset: { y: 2000 },
      contentInset: { top: 0, bottom: nextBottom },
    });
    f.frame();
    assert.deepEqual(f.offsets, [2000, nextBottom === 200 ? 1840 : 2140]);
  });

test('a completed portrait result is realigned after rotation without another query or canonical read', () => {
  const records: Readonly<Record<string, unknown>>[] = [];
  const f = fixture((record) => records.push(record));
  f.ready(geometry());
  f.reveal.request('same-result');
  f.frame();
  assert.ok(records.some((record) => record.event === 'retire'));
  assert.deepEqual(f.offsets, []);
  f.setRow(tallHit);
  f.reveal.onLayout(402);
  f.reveal.onInset(expected470);
  f.reveal.onScroll(native470);
  f.frame();
  assert.deepEqual(f.offsets, [0]);
  f.reveal.onScroll({ ...native470, contentOffset: { y: 0 }, contentInset: expected470 });
  f.commit();
  f.frame();
  assert.equal(records.filter((record) => record.event === 'retire').length, 2);
});

for (const cancel of ['query', 'owner-or-focus', 'drag'] as const)
  test(`${cancel} prevents a settled portrait anchor from scrolling after rotation`, () => {
    const f = fixture();
    f.ready(geometry());
    f.reveal.request('same-result');
    f.frame();
    const completedFrames = f.frames.length;
    if (cancel === 'drag') f.reveal.onScroll(geometry({ responderIgnoreScroll: false }));
    else if (cancel === 'owner-or-focus') f.setCurrent(false);
    else f.reveal.cancel(cancel);
    f.setRow(tallHit);
    f.reveal.onLayout(402);
    f.reveal.onInset(expected470);
    f.reveal.onScroll(native470);
    if (cancel === 'owner-or-focus') {
      f.frame(); // The current-owner guard retires the resumed intent.
      f.setCurrent(true);
      const retiredFrames = f.frames.length;
      f.reveal.onInset({ top: 0, bottom: 0 });
      assert.equal(f.frames.length, retiredFrames, 'a later focus cannot resurrect the old anchor');
    } else assert.equal(f.frames.length, completedFrames, 'retired intent schedules no work');
    assert.deepEqual(f.offsets, []);
  });

test('RN no-op at the same offset and inset-only updates cannot fabricate a completion or a scroll', () => {
  const records: Readonly<Record<string, unknown>>[] = [];
  const f = fixture((record) => records.push(record));
  f.ready({ ...native470, contentOffset: { y: 0 } });
  f.setRow(tallHit);
  f.reveal.onInset(expected470);
  f.reveal.request('already-at-hit');
  f.frame();
  f.commit();
  f.frame();
  assert.deepEqual(f.offsets, [], 'RN returns without an event when the requested offset is equal');
  assert.ok(!records.some((record) => record.event === 'retire'));
  f.reveal.onInset(expected470);
  f.frame();
  assert.deepEqual(f.offsets, []);
  assert.ok(
    !records.some((record) => record.event === 'retire'),
    'effective animatedProps are not applied native metrics',
  );
  f.reveal.onScroll({ ...native470, contentOffset: { y: 0 }, contentInset: expected470 });
  f.frame();
  assert.equal(records.filter((record) => record.event === 'retire').length, 1);
});

for (const cancel of ['query', 'owner-or-focus', 'drag'] as const)
  test(`an inset-only seek cannot cross ${cancel} retirement`, () => {
    const f = fixture();
    f.ready(native470);
    f.reveal.onInset(expected470);
    f.setRow(tallHit);
    f.reveal.request('old-hit');
    const old = f.frames.at(-1)!;
    if (cancel === 'owner-or-focus') f.setCurrent(false);
    else f.reveal.cancel(cancel);
    old.run();
    assert.deepEqual(f.offsets, []);
  });
