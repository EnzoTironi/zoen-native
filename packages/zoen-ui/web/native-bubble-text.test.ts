import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const output = resolve(import.meta.dirname, '../node_modules/.cache/native-bubble-text-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-bubble-text.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic' });
const { nativeLineWidth } = await import(pathToFileURL(output).href);
const rect = (left: number, top: number, width: number, height: number) => ({ left, top, right: left + width, bottom: top + height, width, height });
test('inline metadata reservation shares its text line despite a smaller height', () => {
  assert.equal(nativeLineWidth([rect(10, 0, 90, 20), rect(104, 6, 60, 13)]), 154);
});
test('a wrapped receipt does not force the longest glyph line to the full proposed width', () => {
  assert.equal(nativeLineWidth([rect(14, 0, 249, 20), rect(14, 27, 76, 13)]), 249);
});
test('real DOMRect prototype getters are read explicitly, not lost by object spread', () => {
  const inherited = (left: number, top: number, width: number, height: number) => Object.create(rect(left, top, width, height));
  assert.equal(nativeLineWidth([inherited(14, 0, 249, 20), inherited(14, 27, 76, 13)]), 249);
});

test('empty reservations and blank lines do not create an intrinsic width', () => {
  assert.equal(nativeLineWidth([rect(0, 0, 0, 20), rect(20, 20, 50, 0)]), 0);
});
