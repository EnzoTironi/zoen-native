// node --test src/sky/living-sky.test.mjs : the sky's time-of-day model.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync, statSync } from 'node:fs';
import vm from 'node:vm';

const ctx = {};
ctx.globalThis = ctx;
vm.runInNewContext(readFileSync(new URL('./living-sky.js', import.meta.url), 'utf8'), ctx);
const sky = ctx.ZoenSky;

test('phase names follow local time', () => {
  for (const [h, p] of [[0, 'night'], [4.9, 'night'], [5, 'dawn'], [7.9, 'dawn'], [8, 'day'], [12, 'day'], [16, 'sunset'], [19.9, 'sunset'], [20, 'night'], [23.5, 'night'], [-1, 'night'], [NaN, 'day']]) assert.equal(sky.phase(h), p, String(h));
});

test('weights sum to 1, use at most two clips, and change smoothly every minute of the day', () => {
  let prev = sky.weights(-1 / 60);
  for (let m = 0; m <= 1440; m++) {
    const w = sky.weights(m / 60);
    const vals = Object.values(w);
    assert.ok(Math.abs(vals.reduce((a, b) => a + b, 0) - 1) < 1e-9, `sum at ${m}`);
    assert.ok(vals.filter((v) => v > 0).length <= 2, `count at ${m}`);
    for (const k of Object.keys(w)) assert.ok(Math.abs(w[k] - prev[k]) < 0.06, `${k} jump at ${m}`);
    prev = w;
  }
});

test('each phase is fully shown in the middle of its time', () => {
  for (const [h, p] of [[2, 'night'], [6.5, 'dawn'], [12, 'day'], [18, 'sunset']]) assert.equal(sky.weights(h)[p], 1);
  assert.equal(sky.weights(8).dawn, 0.5);
});

test('every phase has a small video pair and a poster', () => {
  let total = 0;
  for (const p of sky.PHASES) for (const ext of ['webm', 'mp4', 'jpg']) {
    const f = new URL(`../../public/sky/${p}.${ext}`, import.meta.url);
    assert.ok(existsSync(f), `${p}.${ext}`);
    total += statSync(f).size;
  }
  assert.ok(total < 3 * 1024 * 1024, `sky assets are ${total} bytes`);
});
