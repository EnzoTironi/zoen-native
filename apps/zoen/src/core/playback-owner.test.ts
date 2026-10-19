import assert from 'node:assert/strict';
import test from 'node:test';
import { PlaybackCoordinator } from '../media/playback-owner.ts';

function player(coordinator: PlaybackCoordinator, immediate = true) {
  const value = { playing: false, pauses: 0 };
  const owner = coordinator.register(
    () => {
      value.pauses++;
      if (immediate) value.playing = false;
    },
    () => value.playing,
  );
  return { value, owner };
}

test('a native video already playing before its event is paused by a new audio claim', () => {
  const coordinator = new PlaybackCoordinator();
  const video = player(coordinator);
  const audio = player(coordinator);
  video.value.playing = true;
  const intent = audio.owner.begin();
  assert.equal(video.value.pauses, 1);
  video.owner.observe(true); // Queued old event disagrees with actual native state.
  assert.equal(audio.owner.isCurrent(intent), true);
  assert.equal(video.value.playing, false);
});

test('late native playing cannot steal audio while a prior video pause is settling', () => {
  const coordinator = new PlaybackCoordinator();
  const video = player(coordinator, false);
  const audio = player(coordinator);
  video.value.playing = true;
  video.owner.observe(true);
  const intent = audio.owner.begin();
  video.owner.observe(true);
  assert.equal(audio.owner.isCurrent(intent), true);
  assert.equal(video.value.pauses, 2);
  video.value.playing = false;
  video.owner.observe(true); // Actual stopped state clears the block without a false event.
  video.value.playing = true; // A later genuine native/VoiceOver play remains usable.
  video.owner.observe(true);
  assert.equal(audio.owner.isCurrent(intent), false);
});

test('stale native stopped events and old disposal cannot release another current owner', () => {
  const coordinator = new PlaybackCoordinator();
  const video = player(coordinator);
  const audio = player(coordinator);
  video.value.playing = true;
  video.owner.observe(true);
  const current = audio.owner.begin();
  video.owner.observe(false);
  video.owner.dispose();
  video.value.playing = true;
  video.owner.observe(true);
  assert.equal(audio.owner.isCurrent(current), true);
});

test('an already stopped player needs no stop event before a new native play', () => {
  const coordinator = new PlaybackCoordinator();
  const video = player(coordinator);
  coordinator.pauseAll();
  assert.equal(video.value.pauses, 1);
  video.value.playing = true;
  video.owner.observe(true);
  coordinator.pauseAll();
  assert.equal(video.value.playing, false);
});

test('an audio continuation after an await is revoked by a newer video or pause-all intent', async () => {
  const coordinator = new PlaybackCoordinator();
  const audio = player(coordinator);
  const video = player(coordinator);
  let complete!: () => void;
  const ready = new Promise<void>((resolve) => {
    complete = resolve;
  });
  const intent = audio.owner.begin();
  const pending = ready.then(() => {
    if (audio.owner.isCurrent(intent)) audio.value.playing = true;
  });
  video.value.playing = true;
  video.owner.observe(true);
  complete();
  await pending;
  assert.equal(audio.value.playing, false);
  const second = audio.owner.begin();
  coordinator.pauseAll();
  assert.equal(audio.owner.isCurrent(second), false);
});

test('release is exact-intent scoped and replacing a player rejects its late notifications', () => {
  const coordinator = new PlaybackCoordinator();
  const old = player(coordinator);
  const previous = old.owner.begin();
  coordinator.pauseAll();
  const current = old.owner.begin();
  old.owner.release(previous);
  assert.equal(old.owner.isCurrent(current), true);
  old.owner.dispose();
  const replacement = player(coordinator);
  const next = replacement.owner.begin();
  old.value.playing = true;
  old.owner.observe(true);
  assert.equal(replacement.owner.isCurrent(next), true);
});

test('rapid native stop and replay before the stopped callback needs no second user tap', () => {
  const coordinator = new PlaybackCoordinator();
  const video = player(coordinator, false);
  const audio = player(coordinator);
  video.value.playing = true;
  video.owner.observe(true);
  const audioIntent = audio.owner.begin();
  // Native pause and genuine user replay both happen before JS receives false.
  video.value.playing = false;
  video.value.playing = true;
  video.owner.observe(false);
  assert.equal(audio.owner.isCurrent(audioIntent), true);
  video.owner.observe(true);
  assert.equal(audio.owner.isCurrent(audioIntent), false);
  assert.equal(video.value.pauses, 1);
});

test('every explicit audio play invalidates the prior continuation on the same owner', () => {
  const coordinator = new PlaybackCoordinator();
  const audio = player(coordinator);
  const first = audio.owner.begin();
  const second = audio.owner.begin();
  assert.notEqual(first, second);
  assert.equal(audio.owner.isCurrent(first), false);
  assert.equal(audio.owner.isCurrent(second), true);
});

test('a newer audio claim cancels a native video waiting for data before it can resume', () => {
  const coordinator = new PlaybackCoordinator();
  let waitingToPlay = true;
  let playing = false;
  const video = coordinator.register(
    () => {
      waitingToPlay = false;
      playing = false;
    },
    () => playing,
  );
  const audio = player(coordinator);
  const intent = audio.owner.begin();
  assert.equal(waitingToPlay, false);
  // A delayed historical true notification must not turn that canceled intent into a claim.
  video.observe(true);
  assert.equal(audio.owner.isCurrent(intent), true);
  assert.equal(playing, false);
});

test('late SDK audio reset resumption cannot reclaim playback after video or pause-all', () => {
  for (const replacement of ['video', 'pause-all']) {
    const coordinator = new PlaybackCoordinator();
    const audio = player(coordinator);
    const video = player(coordinator);
    const old = audio.owner.begin();
    // SDK saves wasPlaying=true, then reloads with actual playback temporarily false.
    audio.value.playing = true;
    audio.value.playing = false;
    const current = replacement === 'video' ? video.owner.begin() : null;
    if (replacement === 'pause-all') coordinator.pauseAll();
    // The old native restore closure resumes even though pause() was already called.
    audio.value.playing = true;
    audio.owner.observeExplicitPlayback();
    assert.equal(audio.value.playing, false);
    assert.equal(audio.owner.isCurrent(old), false);
    if (current) assert.equal(video.owner.isCurrent(current), true);
  }
});

test('an authorized SDK audio resume preserves its explicit intent and an unowned one never auto-claims', () => {
  const coordinator = new PlaybackCoordinator();
  const audio = player(coordinator);
  const current = audio.owner.begin();
  audio.value.playing = true;
  audio.owner.observeExplicitPlayback();
  assert.equal(audio.value.playing, true);
  assert.equal(audio.owner.isCurrent(current), true);
  coordinator.pauseAll();
  audio.value.playing = true;
  audio.owner.observeExplicitPlayback();
  assert.equal(audio.value.playing, false);
  const next = audio.owner.begin();
  audio.value.playing = true;
  audio.owner.observeExplicitPlayback();
  assert.equal(audio.owner.isCurrent(next), true);
});
