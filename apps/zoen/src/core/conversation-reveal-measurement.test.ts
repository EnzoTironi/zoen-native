import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ConversationRevealMeasurement } from './conversation-reveal-measurement.ts';
import type { ConversationContentLayout } from './conversation-reveal-content.ts';

// Explicit native measurement port: tests validate callback/lifecycle contracts,
// not UIKit pixels. Coordinates are relative to the actual row content host.
export function measurementFixture() {
  const reports: ConversationContentLayout[] = [];
  const pending: ((x: number, y: number, width: number, height: number) => void)[] = [];
  const bodyNode = { measureLayout() {} };
  const textNode = {
    measureLayout(
      relative: unknown,
      success: (x: number, y: number, w: number, h: number) => void,
    ) {
      assert.equal(relative, bodyNode);
      pending.push(success);
    },
  };
  const measurement = new ConversationRevealMeasurement(true, (layout) => reports.push(layout));
  // These objects represent native event ports, not fabricated native API results.
  const body = (width = 842, height = 139) => ({
    currentTarget: bodyNode,
    nativeEvent: { layout: { x: 0, y: 39 + 2 / 3, width, height } },
  });
  const anchor = (width = 266, height = 46) => ({
    currentTarget: textNode,
    nativeEvent: { layout: { x: 14, y: 10, width, height } },
  });
  const lines = (texts: string[] = ['Earlier text ', 'QA345: contact']) => ({
    currentTarget: textNode,
    nativeEvent: {
      lines: texts.map((text, i) => ({ text, x: 0, y: i * 23, width: 200, height: 23 })),
    },
  });
  return { measurement, reports, pending, body, anchor, lines };
}

test('matching native line is measured inside the caption, excluding media and date', () => {
  const f = measurementFixture();
  f.measurement.setQuery('qa345');
  f.measurement.body(f.body() as never);
  f.measurement.anchor(f.anchor() as never);
  f.measurement.text(f.lines() as never);
  f.pending.at(-1)!(14, 73 + 2 / 3, 266, 46);
  assert.deepEqual(f.reports.at(-1)!.anchor, { y: 96 + 2 / 3, width: 200, height: 23 });
  assert.equal(f.reports.at(-1)!.anchorPending, false);
  const count = f.reports.length;
  f.measurement.body(f.body() as never);
  f.measurement.anchor(f.anchor() as never);
  f.measurement.text(f.lines() as never);
  assert.equal(f.reports.length, count, 'unchanged layout cannot replay a settled reveal');
});

test('query replacement rejects old asynchronous measurements and supports wrapped matches', () => {
  const f = measurementFixture();
  f.measurement.setQuery('Earlier');
  f.measurement.body(f.body() as never);
  f.measurement.anchor(f.anchor() as never);
  f.measurement.text(f.lines(['Earlier QA', '345 result']) as never);
  const old = f.pending.at(-1)!;
  f.measurement.setQuery('QA345');
  old(14, 60, 266, 46);
  assert.equal(f.reports.at(-1)!.anchorPending, true);
  f.pending.at(-1)!(14, 60, 266, 46);
  assert.deepEqual(
    f.reports.at(-1)!.anchor,
    { y: 60, width: 200, height: 23 },
    'the first matching rendered line is the target even when the query wraps',
  );
});

test('rotation and retirement reject stale measurements; effect reconnection remains usable', () => {
  const f = measurementFixture();
  f.measurement.body(f.body() as never);
  f.measurement.anchor(f.anchor() as never);
  const old = f.pending.at(-1)!;
  f.measurement.body(f.body(370) as never);
  old(14, 60, 266, 46);
  assert.equal(f.reports.at(-1)!.width, 370);
  assert.equal(f.reports.at(-1)!.anchorPending, true);
  f.pending.at(-1)!(14, 60, 400, 46);
  assert.equal(f.reports.at(-1)!.anchor, undefined, 'outside/rotated host geometry is rejected');
  const beforeRetire = f.pending.at(-1)!;
  f.measurement.retire();
  const count = f.reports.length;
  beforeRetire(14, 60, 266, 46);
  f.measurement.body(f.body() as never);
  assert.equal(f.reports.length, count);
  f.measurement.activate();
  f.pending.at(-1)!(14, 60, 266, 46);
  assert.deepEqual(f.reports.at(-1)!.anchor, { y: 60, width: 266, height: 46 });
});
