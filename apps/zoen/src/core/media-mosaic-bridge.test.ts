import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { mediaMosaicLayout as unavailable } from '../media/mosaic-layout.ts';
const code = (
  await build({
    stdin: {
      contents: readFileSync(new URL('../media/mosaic-layout.ios.ts', import.meta.url), 'utf8'),
      loader: 'ts',
    },
    bundle: false,
    platform: 'node',
    format: 'cjs',
    write: false,
  })
).outputFiles[0]!.text;
function load(native: unknown) {
  const module = {
    exports: {} as { supportsMediaMosaic: boolean; mediaMosaicLayout: typeof unavailable },
  };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: () => ({ default: native, __esModule: true }),
  });
  return module.exports;
}
test('optional iOS mosaic bridge falls back to canonical individual rows without an installed native capability', () => {
  assert.equal(
    unavailable(294, 3, [
      [1, 1],
      [1, 1],
    ]),
    null,
  );
  const old = load(null);
  assert.equal(old.supportsMediaMosaic, false);
  assert.equal(
    old.mediaMosaicLayout(294, 3, [
      [1, 1],
      [1, 1],
    ]),
    null,
  );
});
test('iOS wrapper delegates actual pixel sizes to the native algorithm and refuses malformed or foreign frame counts', () => {
  const sizes = [
    [100, 200],
    [200, 100],
  ] as const;
  const geometry = {
    width: 201,
    height: 100,
    items: [
      { x: 0, y: 0, width: 100, height: 100, position: 1 },
      { x: 101, y: 0, width: 100, height: 100, position: 2 },
    ],
  };
  let arguments_: unknown;
  const f = load({
    attachmentMosaicLayout: (...args: unknown[]) => {
      arguments_ = args;
      return geometry;
    },
  });
  assert.equal(f.supportsMediaMosaic, true);
  assert.strictEqual(f.mediaMosaicLayout(294, 3, sizes), geometry);
  assert.equal(JSON.stringify(arguments_), JSON.stringify([294, 294, 3, sizes]));
  assert.notStrictEqual((arguments_ as unknown[])[3], sizes);
  for (const value of [
    null,
    { ...geometry, items: [] },
    { ...geometry, height: Infinity },
    { ...geometry, items: [geometry.items[0], { ...geometry.items[1], x: 200 }] },
  ]) {
    assert.equal(
      load({ attachmentMosaicLayout: () => value }).mediaMosaicLayout(294, 3, sizes),
      null,
    );
  }
  assert.equal(
    load({
      attachmentMosaicLayout: () => {
        throw new Error('old native unavailable');
      },
    }).mediaMosaicLayout(294, 3, sizes),
    null,
  );
});
