import assert from 'node:assert/strict';
import test from 'node:test';
import { materializeObservedFile } from '../media/observed-file.ts';
import type { LocalMediaFile, MediaDriver } from '../media/types';
import { deferred } from './store.fixtures.ts';

function fixture() {
  const result = deferred<LocalMediaFile | null>();
  const reads: unknown[][] = [],
    releases: string[] = [];
  let active = true;
  const file = {
    itemId: 'photo',
    version: 4,
    uri: 'file:///owned-verified-photo',
    info: { sha256: 'ab'.repeat(32), mime: 'image/png' },
  } as LocalMediaFile;
  const media = {
    materializeFile: async (...args: unknown[]) => {
      reads.push(args);
      return result.promise;
    },
    releaseLocalFile: async (uri: string) => {
      releases.push(uri);
    },
  } as unknown as MediaDriver;
  return {
    file,
    reads,
    releases,
    resolve: result.resolve,
    retire: () => {
      active = false;
    },
    run: () =>
      materializeObservedFile(
        media,
        { itemId: 'photo', version: 4, expectedHash: 'ab'.repeat(32) },
        () => active,
      ),
  };
}

test('existing media loader receives only the observed canonical Item/version and returns its lease', async () => {
  const f = fixture();
  const work = f.run();
  assert.deepEqual(f.reads, [['photo', 4]]);
  f.resolve(f.file);
  assert.equal(await work, f.file);
  assert.deepEqual(f.releases, []);
});

test('retired source never starts a read, and late completion releases its private copy', async () => {
  const before = fixture();
  before.retire();
  await assert.rejects(before.run());
  assert.deepEqual(before.reads, []);
  const after = fixture();
  const work = after.run();
  after.retire();
  after.resolve(after.file);
  await assert.rejects(work);
  assert.deepEqual(after.releases, [after.file.uri]);
});

test('wrong Item, version or hash is released instead of entering the image renderer', async () => {
  for (const patch of [
    { itemId: 'other' },
    { version: 5 },
    { info: { sha256: 'cd'.repeat(32), mime: 'image/png' } },
  ]) {
    const f = fixture();
    const work = f.run();
    f.resolve({ ...f.file, ...patch } as LocalMediaFile);
    await assert.rejects(work);
    assert.deepEqual(f.releases, [f.file.uri]);
  }
});

test('a missing file remains unavailable; no fake URI or retry is created', async () => {
  const f = fixture();
  const work = f.run();
  f.resolve(null);
  await assert.rejects(work);
  assert.equal(f.reads.length, 1);
  assert.deepEqual(f.releases, []);
});
