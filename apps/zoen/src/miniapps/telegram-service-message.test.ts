import assert from 'node:assert/strict';
import { test } from 'node:test';
import { telegramServiceLineRects, telegramServiceMask } from './telegram-service-message.ts';

test('service layout ports the upstream 40pt neighbor rule, 7pt sides and 22pt line mask', () => {
  const native = [
    { x: 0, y: 0, width: 200, height: 16 },
    { x: 19, y: 16, width: 162, height: 16 },
    { x: 70, y: 32, width: 60, height: 16 },
  ];
  const before = JSON.stringify(native);
  assert.deepEqual(telegramServiceLineRects(native, 200), [
    { x: -7, y: -3, width: 214, height: 22 },
    { x: -7, y: 13, width: 214, height: 22 },
    { x: 63, y: 29, width: 74, height: 22 },
  ]);
  assert.equal(JSON.stringify(native), before);
  const boundary = telegramServiceLineRects(
    [
      { x: 0, y: 0, width: 200, height: 16 },
      { x: 20, y: 16, width: 160, height: 16 },
    ],
    200,
  );
  assert.equal(boundary[1]!.width, 174, 'Exactly40pt does not merge widths');
});

test('one line has the exact full-corner mask and transparent fill is applied only once', () => {
  const shape = telegramServiceMask([{ x: -7, y: -3, width: 114, height: 22 }], '#FFFFFFCC')!;
  assert.deepEqual(
    { x: shape.x, y: shape.y, width: shape.width, height: shape.height },
    { x: -7, y: -3, width: 114, height: 22 },
  );
  const svg = decodeURIComponent(shape.uri.split(',')[1]!);
  assert.equal(svg.match(/fill="#FFFFFF"/g)?.length, 1);
  assert.ok(svg.includes('fill-opacity="0.8"'));
  assert.ok(svg.includes('<rect x="0" y="0" width="11" height="11" fill="black"/>'));
  assert.ok(svg.includes('<circle cx="11" cy="11" r="11" fill="white"/>'));
  assert.ok(svg.includes('<circle cx="103" cy="11" r="11" fill="white"/>'));
  assert.ok(svg.includes('mask="url(#m)"'));
});

test('multiline shrinking and expanding masks retain the upstream concave joins', () => {
  for (const widths of [
    [200, 100],
    [100, 200],
    [200, 100, 200],
    [100, 200, 100],
  ]) {
    const rects = telegramServiceLineRects(
      widths.map((width, i) => ({ x: (200 - width) / 2, y: i * 16, width, height: 16 })),
      200,
    );
    const mask = telegramServiceMask(rects, '#1F1F1F')!;
    const svg = decodeURIComponent(mask.uri.split(',')[1]!);
    assert.ok(svg.includes('fill="black"'));
    assert.ok(svg.includes('fill="white"'));
    assert.equal(mask.width, 214);
    assert.equal(mask.height, 22 + 16 * (widths.length - 1));
    assert.ok(!svg.includes('NaN') && !svg.includes('Infinity'));
  }
});

test('large native text keeps complete geometry and invalid measurements or colors produce no stale mask', () => {
  const rects = telegramServiceLineRects([{ x: 0, y: 0, width: 250, height: 49.3 }], 250);
  assert.equal(rects[0]!.height, 50);
  assert.ok(telegramServiceMask(rects, '#1F1F1F'));
  for (const lines of [
    [],
    [{ x: 0, y: 0, width: NaN, height: 16 }],
    [{ x: 0, y: 0, width: 40, height: 0 }],
  ])
    assert.deepEqual(telegramServiceLineRects(lines, 200), []);
  assert.deepEqual(telegramServiceLineRects([{ x: 0, y: 0, width: 40, height: 16 }], Infinity), []);
  assert.equal(telegramServiceMask([], '#FFFFFFCC'), null);
  assert.equal(telegramServiceMask(rects, 'red"/><script/>'), null);
});
