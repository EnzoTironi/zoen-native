import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { VideoThumbnail } from 'expo-video';
import {
  createDraftVideoThumbnails,
  type DraftThumbnailPlayer,
} from '../media/draft-video-thumbnail.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const flush = async () => {
  for (let count = 0; count < 6; count++) await Promise.resolve();
};
function image() {
  const value = {
    releases: 0,
    release() {
      value.releases++;
    },
  };
  return value as typeof value & VideoThumbnail;
}
function fixture(initial: 'loading' | 'readyToPlay' = 'loading') {
  const thumbnail = image();
  const listeners = new Set<(event: { status: string }) => void>();
  const player = {
    status: initial as string,
    releases: 0,
    generations: 0,
    extraction: null as Promise<VideoThumbnail[]> | null,
    addListener(_event: string, listener: (event: { status: string }) => void) {
      listeners.add(listener);
      return {
        remove() {
          listeners.delete(listener);
        },
      };
    },
    async generateThumbnailsAsync(
      times: number[],
      options: { maxWidth: number; maxHeight: number },
    ) {
      player.generations++;
      assert.deepEqual(times, [0]);
      assert.deepEqual(options, { maxWidth: 192, maxHeight: 192 });
      // SDK58 returns [] before the native currentItem is installed.
      if (player.status !== 'readyToPlay') return [];
      return player.extraction ? await player.extraction : [thumbnail];
    },
    release() {
      player.releases++;
    },
    emit(status: string) {
      player.status = status;
      for (const listener of listeners) listener({ status });
    },
  };
  return { player, thumbnail, listeners, port: player as unknown as DraftThumbnailPlayer };
}

test('draft thumbnail waits for SDK58 readiness instead of freezing its initial empty result', async () => {
  const f = fixture();
  const load = createDraftVideoThumbnails(() => f.port);
  const work = load('file:///synthetic/video.mp4', new AbortController().signal);
  await flush();
  assert.equal(f.player.generations, 0);
  assert.equal(f.listeners.size, 1);
  f.player.emit('readyToPlay');
  assert.equal(await work, f.thumbnail);
  assert.equal(f.player.generations, 1);
  assert.equal(f.player.releases, 1);
  assert.equal(f.listeners.size, 0);
});

test('an already-ready local source is sampled after subscription without requiring another event', async () => {
  const f = fixture('readyToPlay');
  assert.equal(
    await createDraftVideoThumbnails(() => f.port)('file:///qa.mp4', new AbortController().signal),
    f.thumbnail,
  );
  assert.equal(f.player.releases, 1);
  assert.equal(f.listeners.size, 0);
});

test('cancel during loading releases its player and a cancelled queued selection never creates one', async () => {
  const f = fixture();
  let created = 0;
  const load = createDraftVideoThumbnails(() => {
    created++;
    return f.port;
  });
  const first = new AbortController();
  const second = new AbortController();
  const a = load('file:///first.mp4', first.signal);
  const b = load('file:///second.mp4', second.signal);
  await flush();
  second.abort();
  first.abort();
  assert.equal(await a, null);
  assert.equal(await b, null);
  assert.equal(created, 1);
  assert.equal(f.player.generations, 0);
  assert.equal(f.player.releases, 1);
  assert.equal(f.listeners.size, 0);
});

test('an admitted cancelled extraction drains before the next decoder and releases its unused image', async () => {
  const a = fixture('readyToPlay');
  const b = fixture('readyToPlay');
  const extraction = deferred<VideoThumbnail[]>();
  a.player.extraction = extraction.promise;
  let created = 0;
  const load = createDraftVideoThumbnails(() => (++created === 1 ? a.port : b.port));
  const canceled = new AbortController();
  const first = load('file:///first.mp4', canceled.signal);
  const second = load('file:///second.mp4', new AbortController().signal);
  await flush();
  canceled.abort();
  await flush();
  assert.equal(created, 1);
  assert.equal(a.player.releases, 0);
  extraction.resolve([a.thumbnail]);
  assert.equal(await first, null);
  assert.equal(await second, b.thumbnail);
  assert.equal(a.thumbnail.releases, 1);
  assert.equal(a.player.releases, 1);
  assert.equal(b.player.releases, 1);
});

test('status failure cleans up and does not poison the next selected video', async () => {
  const a = fixture();
  const b = fixture('readyToPlay');
  let created = 0;
  const load = createDraftVideoThumbnails(() => (++created === 1 ? a.port : b.port));
  const first = load('file:///bad.mp4', new AbortController().signal);
  const rejected = assert.rejects(first, /source failed/);
  await flush();
  a.player.emit('error');
  await rejected;
  assert.equal(await load('file:///good.mp4', new AbortController().signal), b.thumbnail);
  assert.equal(a.player.releases, 1);
  assert.equal(a.listeners.size, 0);
});

test('a source which never loads has a bounded readiness wait and releases its listener', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = fixture();
  const load = createDraftVideoThumbnails(() => f.port);
  const work = load('file:///stalled.mp4', new AbortController().signal);
  const rejected = assert.rejects(work, /source timed out/);
  await flush();
  t.mock.timers.tick(15_000);
  await rejected;
  assert.equal(f.player.releases, 1);
  assert.equal(f.player.generations, 0);
  assert.equal(f.listeners.size, 0);
});
