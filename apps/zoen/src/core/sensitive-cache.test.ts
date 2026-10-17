import assert from 'node:assert/strict';
import test from 'node:test';
import { SensitiveCacheWork } from '../media/cache-work.ts';
import { isAppCacheCopy } from '../media/cache-paths.ts';

test('a cache drain fences new writers and awaits finalizers before clearing', async () => {
  const work = new SensitiveCacheWork();
  const events: string[] = [];
  let finish!: () => void;
  const pending = work.run(() =>
    new Promise<void>((resolve) => {
      events.push('started');
      finish = resolve;
    }).finally(() => {
      work.track(Promise.resolve().then(() => events.push('finalized')));
    }),
  );
  const erase = work.erase(async () => {
    events.push('cleared');
  });
  assert.deepEqual(events, ['started']);
  await assert.rejects(
    work.run(async () => {}),
    /saindo/,
  );
  finish();
  await Promise.all([pending, erase]);
  assert.deepEqual(events, ['started', 'finalized', 'cleared']);
  work.resume();
  assert.equal(await work.run(async () => 'new account'), 'new account');
});

test('failed cache cleanup stays fenced and can retry after a failed SDK writer', async () => {
  const work = new SensitiveCacheWork();
  await assert.rejects(
    work.run(async () => {
      throw new Error('picker failed');
    }),
    /picker failed/,
  );
  await assert.rejects(
    work.erase(async () => {
      throw new Error('disk cache failed');
    }),
    /disk cache failed/,
  );
  await assert.rejects(
    work.run(async () => {}),
    /saindo/,
  );
  let cleared = false;
  await work.erase(async () => {
    cleared = true;
  });
  assert.equal(cleared, true);
  await assert.rejects(
    work.run(async () => {}),
    /saindo/,
  );
  work.resume();
  await work.run(async () => {});
});

test('cache work begins inside the original user gesture', async () => {
  const work = new SensitiveCacheWork();
  let invoked = false;
  const task = work.run(async () => {
    invoked = true;
  });
  assert.equal(invoked, true);
  await task;
});

test('cache-copy ownership excludes photo originals, documents, assets-library and traversal', () => {
  const root = 'file:///app/Library/Caches/';
  for (const uri of [
    'file:///app/Library/Caches/ImagePicker/source.heic',
    'file:///app/Library/Caches/ImageManipulator/output.jpg',
    'file:///app/Library/Caches/DocumentPicker/document.txt',
    'file:///app/Library/Caches/ExpoAudio/recording-uuid.m4a',
  ])
    assert.equal(isAppCacheCopy(uri, root), true, uri);
  for (const uri of [
    'ph://photo-original',
    'assets-library://original',
    'file:///app/Documents/original.jpg',
    'file:///photos/original.jpg',
    'file:///app/Library/Caches-elsewhere/photo.jpg',
    'file:///app/Library/Caches/',
    'file:///app/Library/Caches/ImagePicker/%2e%2e/%2e%2e/Documents/original.jpg',
    'file:///app/Library/Caches/ImagePicker/%2e%2e%2f%2e%2e%2fDocuments/original.jpg',
    'file:///app/Library/Caches/ImagePicker/photo.jpg?original=1',
    'https://example.com/photo.jpg',
    'not a URI',
  ])
    assert.equal(isAppCacheCopy(uri, root), false, uri);
});
