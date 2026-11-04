import assert from 'node:assert/strict';
import test from 'node:test';
import { MediaPlaybackJournal } from '../media/media-playback-journal.ts';
import { fixture, deferred, tick } from './message-audio-session.fixtures.ts';
import type { MediaPlaybackBookmark } from './generated/MediaPlaybackBookmark';
import type { MediaPlaybackPosition } from './generated/MediaPlaybackPosition';

function journal() {
  let current = true,
    sequence = 1,
    value: MediaPlaybackBookmark = {
      revision: 'r1',
      position: { position_ms: 4000, duration_ms: 10000, rate_milli: 1500 },
    };
  let readWait: Promise<void> | null = null;
  let wait: Promise<void> | null = null,
    fail = false;
  const writes: MediaPlaybackPosition[] = [];
  const port = {
    current: () => current,
    read: async () => {
      await readWait;
      return value;
    },
    save: async (_scope: unknown, expected: string, position: MediaPlaybackPosition) => {
      await wait;
      if (fail) throw Error('stale checkpoint');
      assert.equal(expected, value.revision);
      writes.push(position);
      return (value = { revision: `r${++sequence}`, position });
    },
  };
  return {
    resume: new MediaPlaybackJournal(port),
    writes,
    value: () => value,
    retire: () => {
      current = false;
    },
    holdRead: (next: Promise<void> | null) => {
      readWait = next;
    },
    gate: (next: Promise<void> | null) => {
      wait = next;
    },
    fail: () => {
      fail = true;
    },
  };
}
test('cold voice playback restores position/rate after verification and does not play merely by mounting', async () => {
  const j = journal(),
    f = fixture(j.resume),
    source = f.source(),
    channel = f.session.channel(source);
  const detach = channel.subscribe(() => {});
  assert.equal(f.natives.length, 0);
  assert.equal(j.writes.length, 0);
  await channel.playFrom();
  await tick();
  await tick();
  assert.equal(f.natives[0].seconds, 4);
  assert.equal(f.natives[0].rate, 1.5);
  f.natives[0].seconds = 7;
  f.natives[0].observe();
  channel.pause();
  await j.resume.flush();
  assert.equal(j.value().position?.position_ms, 7000);
  detach();
  await f.session.dispose();
});
test('checkpoint writer coalesces playback ticks and flushes the latest paused seek with exact CAS revisions', async () => {
  const j = journal(),
    f = fixture(),
    source = f.source();
  await j.resume.load(source, 1);
  const gate = deferred<void>();
  j.gate(gate.promise);
  for (let seconds = 5; seconds <= 9; seconds++)
    j.resume.remember(source, { seconds, duration: 10, rate: 1 }, false);
  j.resume.remember(source, { seconds: 9.5, duration: 10, rate: 2 }, true);
  gate.resolve();
  await j.resume.flush();
  assert.ok(j.writes.length <= 2);
  assert.equal(j.value().position?.position_ms, 9500);
  assert.equal(j.value().position?.rate_milli, 2000);
  await f.session.dispose();
});
test('late restore is ignored after retirement and uncertain saves never automatically overwrite another player', async () => {
  const j = journal(),
    f = fixture(),
    source = f.source();
  await j.resume.load(source, 1);
  j.fail();
  j.resume.remember(source, { seconds: 7, duration: 10, rate: 1 }, true);
  await j.resume.flush();
  j.resume.remember(source, { seconds: 8, duration: 10, rate: 1 }, true);
  await j.resume.flush();
  assert.equal(j.writes.length, 0);
  const read = deferred<void>();
  j.holdRead(read.promise);
  const loading = j.resume.load(source, 1);
  j.retire();
  read.resolve();
  assert.equal(await loading, null);
  await f.session.dispose();
});

test('an older held read cannot replace a newer checkpoint writer for the same source', async () => {
  const f = fixture(),
    source = f.source(),
    old = deferred<MediaPlaybackBookmark>();
  let reads = 0;
  const writes: string[] = [];
  const resume = new MediaPlaybackJournal({
    current: () => true,
    read: () =>
      ++reads === 1
        ? old.promise
        : Promise.resolve({
            revision: 'new-revision',
            position: { position_ms: 7000, duration_ms: 10000, rate_milli: 1000 },
          }),
    save: async (_scope, revision, position) => {
      writes.push(revision);
      return { revision: 'accepted', position };
    },
  });
  const first = resume.load(source, 1);
  await tick();
  assert.equal((await resume.load(source, 1))?.seconds, 7);
  old.resolve({
    revision: 'old-revision',
    position: { position_ms: 1000, duration_ms: 10000, rate_milli: 1000 },
  });
  assert.equal(await first, null);
  resume.remember(source, { seconds: 8, duration: 10, rate: 1 }, true);
  await resume.flush();
  assert.deepEqual(writes, ['new-revision']);
  await f.session.dispose();
});
