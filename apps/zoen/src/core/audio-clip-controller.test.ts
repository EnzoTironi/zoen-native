import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AudioClipPort } from '../media/audio-clip-controller.ts';
import type * as AudioClipModule from '../media/audio-clip-controller.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';

const { AudioClipController } = (await import(
  process.env.ZOEN_AUDIO_CLIP_CONTROLLER_SOURCE ??
    new URL('../media/audio-clip-controller.ts', import.meta.url).href
)) as typeof AudioClipModule;

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture(coordinator = new PlaybackCoordinator()) {
  const controller = new AudioClipController(10_000, coordinator);
  const native = {
    playing: false,
    seconds: 0,
    rate: 1,
    plays: 0,
    pauses: 0,
    reloads: 0,
    seeks: [] as number[],
    modes: 0,
    released: false,
  };
  let mode: () => Promise<void> = async () => {};
  let seek: (seconds: number) => Promise<void> = async () => {};
  const alive = () => assert.equal(native.released, false, 'never address a released SharedObject');
  const port: AudioClipPort = {
    playing() {
      alive();
      return native.playing;
    },
    currentSeconds() {
      alive();
      return native.seconds;
    },
    play() {
      alive();
      native.plays++;
      native.playing = true;
    },
    pause() {
      alive();
      native.pauses++;
      native.playing = false;
    },
    async seek(seconds) {
      alive();
      native.seeks.push(seconds);
      await seek(seconds);
      native.seconds = seconds;
    },
    setRate(rate) {
      alive();
      native.rate = rate;
    },
    reload() {
      alive();
      native.reloads++;
    },
    async preparePlayback() {
      alive();
      native.modes++;
      await mode();
    },
  };
  let disconnect = controller.connect(port);
  const status = (overrides = {}) =>
    controller.observe({
      isLoaded: true,
      playing: native.playing,
      isBuffering: false,
      currentTime: native.seconds,
      duration: 10,
      playbackRate: native.rate,
      ...overrides,
    });
  status();
  return {
    controller,
    native,
    status,
    coordinator,
    mode: (value: typeof mode) => {
      mode = value;
    },
    seek: (value: typeof seek) => {
      seek = value;
    },
    disconnect: () => disconnect(),
    reconnect: () => {
      disconnect = controller.connect(port);
      status();
    },
  };
}

test('play reports pending immediately, seeks precisely and publishes only actual native playback', async () => {
  const f = fixture();
  const mode = deferred();
  const seek = deferred();
  f.mode(() => mode.promise);
  f.seek(() => seek.promise);
  const play = f.controller.playFrom(2.125);
  assert.equal(f.controller.getSnapshot().busy, true);
  assert.equal(f.controller.getSnapshot().currentSeconds, 2.125);
  await tick();
  assert.equal(f.native.modes, 1);
  assert.deepEqual(f.native.seeks, []);
  mode.resolve();
  await tick();
  assert.deepEqual(f.native.seeks, [2.125]);
  assert.equal(f.native.plays, 0);
  seek.resolve();
  await play;
  assert.equal(f.native.plays, 1);
  assert.equal(f.controller.getSnapshot().playing, true);
  assert.equal(f.controller.getSnapshot().busy, false);
});

test('pause-all permanently cancels delayed mode and paused seek through eligibility ABA', async () => {
  for (const stage of ['mode', 'seek']) {
    const f = fixture();
    const gate = deferred();
    let eligible = true;
    f.controller.configure(true, () => eligible, 10_000);
    if (stage === 'mode') f.mode(() => gate.promise);
    else f.seek(() => gate.promise);
    const play = f.controller.playFrom(4);
    await tick();
    eligible = false;
    f.coordinator.pauseAll();
    eligible = true;
    gate.resolve();
    await play;
    await tick();
    assert.equal(f.native.plays, 0, `no resurrection after ${stage}`);
    assert.equal(f.controller.getSnapshot().busy, false);
    await f.controller.playFrom(1);
    assert.equal(f.native.plays, 1, 'a new explicit tap remains usable');
  }
});

test('rapid seek serializes native work and coalesces obsolete targets without an old finally clearing busy', async () => {
  const f = fixture();
  const first = deferred();
  const last = deferred();
  f.seek((seconds) => (seconds === 1 ? first.promise : last.promise));
  const a = f.controller.seekSeconds(1);
  await tick();
  const b = f.controller.seekSeconds(2);
  const c = f.controller.seekSeconds(3);
  await a;
  await b;
  assert.deepEqual(f.native.seeks, [1]);
  assert.equal(f.controller.getSnapshot().currentSeconds, 3);
  first.resolve();
  await tick();
  assert.deepEqual(f.native.seeks, [1, 3]);
  assert.equal(f.controller.getSnapshot().busy, true);
  assert.equal(f.controller.getSnapshot().currentSeconds, 3);
  last.resolve();
  await c;
  assert.equal(f.controller.getSnapshot().currentSeconds, 3);
  assert.equal(f.native.plays, 0, 'audition seek never implicitly starts playback');
});

test('another native owner revokes pending play and late SDK auto-resume cannot steal the new intent', async () => {
  const coordinator = new PlaybackCoordinator();
  const a = fixture(coordinator);
  const b = fixture(coordinator);
  const gate = deferred();
  a.seek(() => gate.promise);
  const old = a.controller.playFrom(2);
  await tick();
  await b.controller.playFrom();
  gate.resolve();
  await old;
  await tick();
  assert.equal(a.native.plays, 0);
  assert.equal(b.native.playing, true);
  a.native.playing = true;
  a.status();
  assert.equal(a.native.playing, false);
  assert.equal(b.native.playing, true);
});

test('native errors expose retry without autoplay and obsolete failures cannot replace a new command', async () => {
  const f = fixture();
  const failure = new Error('mode unavailable');
  f.mode(async () => {
    throw failure;
  });
  await assert.rejects(f.controller.playFrom(), failure);
  assert.equal(f.controller.getSnapshot().error, failure);
  assert.equal(f.controller.getSnapshot().busy, false);
  f.status({ isLoaded: false, error: 'decode failed' });
  assert.equal(f.controller.getSnapshot().loadFailed, true);
  f.controller.retry();
  assert.equal(f.native.reloads, 1);
  assert.equal(f.native.plays, 0);
  assert.equal(f.controller.getSnapshot().loaded, false);
  assert.equal(f.controller.getSnapshot().buffering, true);
  f.status();
  const old = deferred();
  f.mode(() => old.promise);
  const pending = f.controller.playFrom();
  await tick();
  f.controller.invalidate();
  f.mode(async () => {});
  const fresh = f.controller.playFrom(6);
  old.reject(new Error('late old failure'));
  await pending;
  await fresh;
  assert.equal(f.controller.getSnapshot().error, null);
  assert.equal(f.native.seconds, 6);
});

test('layout disconnect fences native work before release and supports effect cleanup/setup reuse', async () => {
  const f = fixture();
  const gate = deferred();
  f.mode(() => gate.promise);
  const pending = f.controller.playFrom(1);
  await tick();
  f.disconnect();
  assert.ok(f.native.pauses > 0);
  f.native.released = true;
  gate.resolve();
  await pending;
  await tick();
  assert.equal(f.native.plays, 0);
  f.controller.invalidate();
  f.controller.retry();
  f.native.released = false;
  f.mode(async () => {});
  f.reconnect();
  await f.controller.playFrom(2);
  assert.equal(f.native.plays, 1);
});

test('invalid seeks/rates do nothing, finite seeks clamp, and replay at the actual end restarts', async () => {
  const f = fixture();
  await f.controller.seekSeconds(Number.NaN);
  assert.deepEqual(f.native.seeks, []);
  await f.controller.seekSeconds(-20);
  await f.controller.seekSeconds(50);
  assert.deepEqual(f.native.seeks, [0, 10]);
  f.controller.setRate(1.5);
  assert.equal(f.native.rate, 1.5);
  f.controller.setRate(9 as 2);
  assert.equal(f.native.rate, 1.5);
  await f.controller.playFrom();
  assert.equal(f.native.seeks.at(-1), 0);
});

test('retired or ineligible controls cannot pause another current playback owner', async () => {
  for (const retired of [false, true]) {
    const coordinator = new PlaybackCoordinator();
    const a = fixture(coordinator);
    const b = fixture(coordinator);
    await a.controller.playFrom();
    if (retired) a.disconnect();
    else a.controller.configure(false);
    await b.controller.playFrom();
    const pauses = b.native.pauses;
    a.controller.pause();
    assert.equal(b.native.playing, true);
    assert.equal(b.native.pauses, pauses);
  }
});
