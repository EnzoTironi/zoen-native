// node --test src/sky/living-sky.test.mjs : the site's sky model matches the app's SkyClock contract.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const ctx = { globalThis: {} };
ctx.globalThis = ctx;
vm.runInNewContext(readFileSync(new URL('./living-sky.js', import.meta.url), 'utf8'), ctx);
const sky = ctx.ZoenSky;
const plain = (v) => JSON.parse(JSON.stringify(v));

test('anchors and palette match the app (PR #127, commit 3d86d3b)', () => {
  assert.deepEqual([...sky.HOURS], [0, 6, 9, 12, 17, 20, 24]);
  assert.deepEqual(plain(sky.COLORS), [[.20,.23,.46],[.84,.48,.53],[.38,.67,.87],[.27,.62,.90],[.94,.61,.30],[.51,.34,.63],[.20,.23,.46]]);
  assert.deepEqual([...sky.STARS], [1, 0, 0, 0, 0, .4, 1]);
});

test('anchors are hit exactly and the curve is continuous, including midnight', () => {
  sky.HOURS.slice(0, -1).forEach((h, i) => assert.deepEqual([...sky.sample(h).rgb], [...sky.COLORS[i]]));
  for (const h of sky.HOURS) {
    const a = sky.sample(h - 1e-6), b = sky.sample(h + 1e-6);
    a.rgb.forEach((v, i) => assert.ok(Math.abs(v - b.rgb[i]) < 1e-6));
    assert.ok(Math.abs(a.stars - b.stars) < 1e-6);
  }
});

test('invalid and negative hours wrap like the app', () => {
  assert.deepEqual([...sky.sample(NaN).rgb], [...sky.sample(12).rgb]);
  assert.deepEqual([...sky.sample(-1).rgb], [...sky.sample(23).rgb]);
  for (let m = 0; m < 1440; m++) {
    const s = sky.sample(m / 60);
    assert.ok(s.rgb.every((v) => v >= 0 && v <= 1) && s.stars >= 0 && s.stars <= 1);
  }
});

test('phase names', () => {
  assert.equal(sky.phase(6), 'dawn');
  assert.equal(sky.phase(12), 'day');
  assert.equal(sky.phase(17.5), 'sunset');
  assert.equal(sky.phase(0), 'night');
  assert.equal(sky.phase(22), 'night');
});

test('the contrast guard holds for every minute of the day in both themes', () => {
  for (const theme of ['light', 'dark']) {
    const { base, max, ok } = sky.THEMES[theme];
    for (let m = 0; m < 1440; m++) {
      const { rgb } = sky.sample(m / 60);
      const w = sky.wash(rgb, theme);
      assert.ok(w >= 0 && w <= max);
      assert.ok(ok(sky.luminance(sky.mix(rgb.map((v) => v * 255), base, w))), `${theme} ${m}`);
    }
  }
});
