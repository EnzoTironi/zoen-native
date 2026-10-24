import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  nativeLocationPoint,
  nativeLocationReader,
  nativeLocationSearch,
} from '../media/native-location.ts';
import type { LocalMediaFile } from '../media/types.ts';

test('location bridge binds the exact held file and rejects malformed native metadata', async () => {
  const file = {
    uri: 'file:///held/location.geojson',
    itemId: 'item',
    version: 7,
    info: { sha256: 'a'.repeat(64) },
  } as LocalMediaFile;
  const calls: unknown[] = [];
  const reader = nativeLocationReader({
    async readLocationFile(request) {
      calls.push(JSON.parse(request));
      return JSON.stringify({ longitude: -180, latitude: 90, label: 'Café 東京' });
    },
  });
  assert.deepEqual(await reader(file), { longitude: -180, latitude: 90, label: 'Café 東京' });
  assert.deepEqual(calls, [{ uri: file.uri, itemId: 'item', version: 7, sha256: 'a'.repeat(64) }]);
  const refusal = Error('native-authority-refused');
  await assert.rejects(
    nativeLocationReader({
      async readLocationFile() {
        throw refusal;
      },
    })(file),
    (error) => error === refusal,
  );
  for (const value of [
    null,
    [],
    {},
    { longitude: true, latitude: 0 },
    { longitude: Infinity, latitude: 0 },
    { longitude: 181, latitude: 0 },
    { longitude: 0, latitude: -91 },
    { longitude: 0, latitude: 0, label: null },
    { longitude: 0, latitude: 0, label: 'é'.repeat(129) },
    { longitude: 0, latitude: 0, label: 'x\n' },
    { longitude: 0, latitude: 0, url: 'https://invalid.test' },
  ]) {
    assert.throws(() => nativeLocationPoint(value));
  }
  assert.deepEqual(nativeLocationPoint({ longitude: 180, latitude: -90 }), {
    longitude: 180,
    latitude: -90,
  });
});

test('search adapter never starts implicitly; exact queries and cancellation retain the binding', async () => {
  const calls: unknown[] = [],
    cancels: string[] = [];
  const search = nativeLocationSearch({
    async searchLocations(request) {
      calls.push(JSON.parse(request));
      return '[{"longitude":1,"latitude":2}]';
    },
    async cancelLocationSearch(binding) {
      cancels.push(binding);
    },
  });
  assert.equal(calls.length, 0);
  for (const query of ['', ' ', 'a\n', 'é'.repeat(129)])
    await assert.rejects(search.searchLocations('binding', query));
  assert.equal(calls.length, 0);
  assert.deepEqual(await search.searchLocations('binding', 'Actual place'), [
    { longitude: 1, latitude: 2 },
  ]);
  assert.deepEqual(calls, [{ binding: 'binding', query: 'Actual place' }]);
  search.cancelLocationSearch('binding');
  assert.deepEqual(cancels, ['binding']);
  const destroyed = nativeLocationSearch({
    async searchLocations() {
      return '[]';
    },
    cancelLocationSearch() {
      throw new Error('module-destroyed');
    },
  });
  assert.doesNotThrow(() => destroyed.cancelLocationSearch('retired'));
  assert.equal(nativeLocationPoint({ longitude: 0, latitude: 0, label: '👩‍👩‍👧‍👦' }).label, '👩‍👩‍👧‍👦');
  const oversize = nativeLocationSearch({
    async searchLocations() {
      return JSON.stringify(Array.from({ length: 21 }, () => ({ longitude: 1, latitude: 2 })));
    },
    async cancelLocationSearch() {},
  });
  await assert.rejects(oversize.searchLocations('binding', 'place'));
});
