import { test } from 'node:test';
import assert from 'node:assert/strict';
import { skySample } from './living-sky.ts';

test('sky is continuous at all anchors including midnight, and wraps negative hours', () => {
  for (const h of [0, 6, 9, 12, 17, 20, 24]) {
    const a = skySample(h - 1e-5), b = skySample(h + 1e-5);
    a.rgb.forEach((v, i) => assert.ok(Math.abs(v - b.rgb[i]) < 1e-8));
    assert.ok(Math.abs(a.stars - b.stars) < 1e-8);
  }
  assert.deepEqual(skySample(-1), skySample(23));
  assert.deepEqual(skySample(NaN), skySample(12));
  assert.equal(skySample(0).stars, 1);
  assert.equal(skySample(12).stars, 0);
});

test('primary and secondary ink meet AA across every minute, both schemes', () => {
  const luminance = (rgb: number[]) => rgb.map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4).reduce((v, c, i) => v + c * [.2126,.7152,.0722][i], 0);
  for (const dark of [false, true]) {
    const base = dark ? [.035,.05,.08] : [.96,.97,.985];
    // Existing shared palette's primary + muted text.
    const inks = dark ? [[238,242,250], [147,160,184]] : [[14,19,32], [91,100,120]];
    for (let minute = 0; minute < 1440; minute++) {
      const sky = skySample(minute / 60);
      const wash = sky.rgb.map((v, i) => base[i] * .92 + v * .08);
      for (const ink of inks) for (const end of [wash, base]) {
        // Include worst-case cloud + star overlap, rather than just bare gradient.
        const bg = dark ? end.map(v => (v * .975 + .025) * .92 + .08) : end.map((v, i) => v * .975 + [.32,.37,.5][i] * .025);
        const a = luminance(bg), b = luminance(ink.map(v => v / 255));
        assert.ok((Math.max(a,b) + .05) / (Math.min(a,b) + .05) >= 4.5, `${dark} at ${minute}`);
      }
    }
  }
});
