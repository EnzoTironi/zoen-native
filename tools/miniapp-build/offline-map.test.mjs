import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';

const bundled = await build({
  stdin: { contents: "export * from '../../miniapps/hike/src/mapGeometry'; export { TRAILS } from '../../miniapps/hike/src/trails';", resolveDir: fileURLToPath(new URL('.', import.meta.url)), loader: 'ts' },
  bundle: true, write: false, format: 'esm', platform: 'node', logLevel: 'silent',
});
const { fitRoute, moveCamera, initialCamera, gesture, TRAILS } = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);

test('every real trail fits above the cards in portrait, landscape and small windows', () => {
  for (const size of [{ width: 360, height: 720 }, { width: 720, height: 360 }, { width: 320, height: 480 }]) {
    for (const trail of TRAILS) {
      const points = fitRoute(trail.line, size);
      assert.equal(points.length, trail.line.length);
      assert.ok(points.every(p => p.x >= 49 && p.x <= size.width - 49 && p.y >= 0 && p.y <= size.height * .68));
      assert.ok(points.every(p => Number.isFinite(p.x) && Number.isFinite(p.y)));
    }
  }
});

test('Mercator is north-up and preserves route proportions', () => {
  const points = fitRoute([[0, 0], [1, 0], [1, 1]], { width: 500, height: 800 });
  assert.ok(points[1].x > points[0].x);
  assert.ok(points[2].y < points[1].y);
  const east = points[1].x - points[0].x, north = points[1].y - points[2].y;
  assert.ok(Math.abs(east / north - 1) < .001);
  assert.deepEqual(fitRoute([], { width: 360, height: 720 }), []);
  assert.ok(fitRoute([[0, 90], [0, -90]], { width: 360, height: 720 }).every(p => Number.isFinite(p.y)));
});

test('pinch and pan keep the route under the finger at both zoom limits', () => {
  const size = { width: 360, height: 720 }, before = { x: 80, y: 280 }, after = { x: 120, y: 300 };
  for (const factor of [.001, 2, 100]) {
    const camera = moveCamera(initialCamera, size, before, after, factor);
    assert.ok(camera.zoom >= .5 && camera.zoom <= 8);
    assert.equal(size.width / 2 + (before.x - size.width / 2) * camera.zoom + camera.x, after.x);
    assert.equal(size.height / 2 + (before.y - size.height / 2) * camera.zoom + camera.y, after.y);
  }
  assert.deepEqual(moveCamera(initialCamera, size, before, after), { zoom: 1, x: 40, y: 20 });
  assert.deepEqual(gesture([{ x: 10, y: 10 }, { x: 30, y: 10 }]), { centre: { x: 20, y: 10 }, distance: 20 });
  assert.equal(gesture([]), null);
});
