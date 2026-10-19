import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  arcPath,
  canvasPoint,
  GLOBE_RADIUS,
  GLOBE_SIDE,
  graticulePath,
  inverse,
  landPath,
  normalizeLongitude,
  project,
  rotate,
  slerp,
  visiblePath,
} from '../src/globe.ts';

const near = (actual, expected, tolerance = 1e-8) =>
  assert.ok(Math.abs(actual - expected) < tolerance, `${actual} != ${expected}`);
const original = readFileSync(
  new URL('../../../apple/Shared/Resources/land110.json', import.meta.url),
);
const embedded = readFileSync(new URL('../src/land110.json', import.meta.url));
const land = JSON.parse(embedded);
const center = { lat: 0, lon: 0 };

function assertPathWithinDisc(path) {
  assert.doesNotMatch(path, /NaN|Infinity/);
  for (const match of path.matchAll(/[ML](-?[\d.]+),(-?[\d.]+)/g)) {
    const x = Number(match[1]),
      y = Number(match[2]);
    assert.ok(Math.hypot(x - GLOBE_SIDE / 2, y - GLOBE_SIDE / 2) <= GLOBE_RADIUS + 0.002);
  }
}

test('orthographic projection distinguishes front, back and the horizon with native axis orientation', () => {
  assert.deepEqual(project(center, center), { x: 0, y: 0, z: 1 });
  const east = project({ lat: 0, lon: 90 }, center);
  near(east.x, 1);
  near(east.y, 0);
  near(east.z, 0);
  const north = project({ lat: 90, lon: 0 }, center);
  near(north.x, 0);
  near(north.y, 1);
  near(project({ lat: 0, lon: 180 }, center).z, -1);
  assert.ok(canvasPoint({ lat: 30, lon: 0 }, center).y < GLOBE_SIDE / 2);
});

test('tap inversion round-trips front landmarks across rotated and antimeridian views', () => {
  for (const view of [
    { lat: 25, lon: 10 },
    { lat: -35, lon: 170 },
    { lat: 79, lon: -160 },
  ]) {
    for (const point of [
      view,
      { lat: 0, lon: 0 },
      { lat: -22.9, lon: -43.2 },
      { lat: 35.7, lon: 139.7 },
      { lat: -33.9, lon: 151.2 },
      { lat: 65, lon: -175 },
    ]) {
      const projected = project(point, view);
      if (projected.z < 0.01) continue;
      const restored = inverse(projected.x, projected.y, view);
      assert.ok(restored);
      near(restored.lat, point.lat);
      near(normalizeLongitude(restored.lon - point.lon), 0);
    }
  }
  const across = inverse(project({ lat: 0, lon: -175 }, { lat: 0, lon: 170 }).x, 0, {
    lat: 0,
    lon: 170,
  });
  near(across.lon, -175);
});

test('outside-disc and nonfinite taps cannot become a geographical guess', () => {
  assert.equal(inverse(1.00001, 0, center), null);
  assert.equal(inverse(1, 1, center), null);
  assert.equal(inverse(Number.NaN, 0, center), null);
  assert.equal(inverse(0, Infinity, center), null);
  const copy = inverse(0, 0, center);
  assert.deepEqual(copy, center);
  assert.notEqual(copy, center);
});

test('drag rotation uses the native gesture scale, clamps latitude and wraps longitude', () => {
  assert.deepEqual(rotate({ lat: 25, lon: 10 }, 360, 0, 360), { lat: 25, lon: -150 });
  assert.deepEqual(rotate({ lat: 25, lon: 170 }, -90, 360, 360), { lat: 80, lon: -150 });
  assert.equal(rotate(center, 0, -1000, 360).lat, -80);
  assert.deepEqual(rotate(center, 30, 30, 0), center);
});

test('great-circle interpolation takes the short dateline route and handles equal or antipodal guesses', () => {
  const a = { lat: 0, lon: 170 },
    b = { lat: 0, lon: -170 };
  near(Math.abs(slerp(a, b, 0.5).lon), 180);
  assert.deepEqual(slerp(a, b, 0), a);
  assert.deepEqual(slerp(a, b, 1), b);
  assert.deepEqual(slerp(a, a, 0.5), a);
  for (let index = 0; index <= 64; index++) {
    const point = slerp(center, { lat: 0, lon: 180 }, index / 64);
    assert.ok(Number.isFinite(point.lat) && Number.isFinite(point.lon));
    near(Math.hypot(...Object.values(project(point, center))), 1);
  }
});

test('land is the actual existing public-domain asset and horizon clipping never projects outside the disc', () => {
  assert.ok(
    original.equals(embedded),
    'do not replace the real coastline with schematic/demo geography',
  );
  assert.ok(land.length > 100);
  const front = landPath([[-10, -10, 10, -10, 0, 10, -10, -10]], center);
  assert.ok(front.endsWith('Z'));
  assert.equal(landPath([[170, -10, -170, -10, 180, 10]], center), '');
  for (const view of [
    center,
    { lat: 25, lon: 10 },
    { lat: 80, lon: 170 },
    { lat: -80, lon: -175 },
  ]) {
    const path = landPath(land, view);
    assert.ok(path.length > 1000);
    assertPathWithinDisc(path);
    assertPathWithinDisc(graticulePath(view));
  }
  assert.notEqual(landPath(land, center), landPath(land, { lat: 0, lon: 90 }));
});

test('grid and result arcs split at the hidden hemisphere instead of drawing a false cross-globe chord', () => {
  const path = visiblePath(
    [{ lat: 0, lon: -60 }, center, { lat: 0, lon: 150 }, { lat: 30, lon: 45 }],
    center,
  );
  assert.equal((path.match(/M/g) ?? []).length, 2);
  assert.equal((path.match(/L/g) ?? []).length, 1);
  const a = { lat: -20, lon: -60 },
    b = { lat: 55, lon: 45 };
  const arc = arcPath(a, b, slerp(a, b, 0.5));
  assert.ok((arc.match(/L/g) ?? []).length > 40);
  assertPathWithinDisc(arc);
  assertPathWithinDisc(arcPath(center, { lat: 0, lon: 180 }, center));
});
