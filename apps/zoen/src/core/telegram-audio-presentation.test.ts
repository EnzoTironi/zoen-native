import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  audioWaveformBars,
  recordingLockProgress,
  recordingMove,
  recordingRelease,
} from './telegram-audio-presentation.ts';
import { VoiceHoldIntent } from '../media/voice-hold-intent.ts';
const tick = () => new Promise<void>((done) => setImmediate(done));
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('mic move and release preserve exact threshold edges, dominant direction and velocity', () => {
  assert.equal(recordingMove(-150, 0), null);
  assert.equal(recordingMove(-150.01, 0), 'cancel');
  assert.equal(recordingMove(0, -110), null);
  assert.equal(recordingMove(0, -110.01), 'lock');
  assert.equal(recordingRelease(-100, 0, 0, 0), 'review');
  assert.equal(recordingRelease(-100.01, 0, 0, 0), 'cancel');
  assert.equal(recordingRelease(0, -60, 0, 0), 'review');
  assert.equal(recordingRelease(0, -60.01, 0, 0), 'lock');
  assert.equal(recordingRelease(-90, -100, 0, 0), 'lock');
  assert.equal(recordingRelease(-100, -90, 0, 0), 'review');
  assert.equal(recordingRelease(0, 0, -400, 0), 'review');
  assert.equal(recordingRelease(0, 0, -401, 0), 'cancel');
  assert.equal(recordingRelease(0, 0, 0, -401), 'lock');
  assert.deepEqual([-10, 0, -52.5, -105, -500].map(recordingLockProgress), [
    10 / 105,
    0,
    0.5,
    1,
    1,
  ]);
});

test('waveform bins are actual per-pixel maxima,2pt bars/1pt gaps and no invented samples', () => {
  assert.deepEqual(audioWaveformBars(2, [1]), []);
  assert.deepEqual(audioWaveformBars(12, []), [2, 2, 2, 2]);
  assert.deepEqual(audioWaveformBars(12, [0, 0.5, 1, 0]), [2, 5, 11, 2]);
  assert.equal(audioWaveformBars(6, [0.2, 1, 0, 0.3])[0], 11);
  assert.ok(Math.abs(audioWaveformBars(6, [0.2, 1, 0, 0.3])[1]! - 2.6) < 1e-10);
  assert.deepEqual(audioWaveformBars(12, [NaN, Infinity, -1, 0]), [2, 2, 2, 2]);
  assert.equal(audioWaveformBars(14, [1]).length, 4);
  assert.deepEqual(
    audioWaveformBars(12, [1]),
    [11, 2, 2, 2],
    'one source sample is never duplicated into fake sound',
  );
});

test('release synchronously reaches native-owner drain before permission resolves, exactly once', async () => {
  const ready = deferred<boolean>();
  const events: string[] = [];
  const intent = new VoiceHoldIntent({
    start: () => {
      events.push('start-admitted');
      return ready.promise;
    },
    review: async () => {
      events.push('drain-start');
      await ready.promise;
      events.push('review');
    },
    cancel: async () => {
      events.push('cancel');
    },
    locked: () => events.push('lock'),
    error: () => events.push('error'),
  });
  intent.begin();
  intent.end('review');
  intent.end('cancel');
  assert.deepEqual(events, ['start-admitted', 'drain-start']);
  ready.resolve(true);
  await tick();
  assert.deepEqual(events, ['start-admitted', 'drain-start', 'review']);
});

test('lock never sends/stops and retirement prevents late gesture work from crossing owners', async () => {
  const events: string[] = [];
  const intent = new VoiceHoldIntent({
    start: async () => true,
    review: async () => {
      events.push('review');
    },
    cancel: async () => {
      events.push('cancel');
    },
    locked: () => events.push('lock'),
    error: () => events.push('error'),
  });
  intent.begin();
  intent.end('lock');
  intent.end('review');
  await tick();
  assert.deepEqual(events, ['lock']);
  intent.completed();
  intent.begin();
  intent.end('cancel');
  await tick();
  assert.deepEqual(events, ['lock', 'cancel']);
  intent.retire();
  intent.begin();
  intent.end('review');
  await tick();
  assert.deepEqual(events, ['lock', 'cancel']);
});

test('a failed old start cannot retire a subsequent attempt or report its error after retirement', async () => {
  let fail!: (reason: unknown) => void;
  const old = new Promise<boolean>((_, reject) => {
    fail = reject;
  });
  const events: string[] = [];
  let starts = 0;
  const intent = new VoiceHoldIntent({
    start: () => (++starts === 1 ? old : Promise.resolve(true)),
    review: async () => {
      events.push('review');
    },
    cancel: async () => {},
    locked: () => events.push('lock'),
    error: () => events.push('error'),
  });
  intent.begin();
  intent.completed();
  intent.begin();
  fail(Error('old'));
  await tick();
  intent.end('review');
  await tick();
  assert.deepEqual(events, ['review']);
  intent.retire();
  intent.activate();
  intent.begin();
  intent.end('lock');
  await tick();
  assert.deepEqual(
    events,
    ['review', 'lock'],
    'effect replay reactivates only presentation admission',
  );
});
