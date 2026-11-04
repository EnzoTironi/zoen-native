import assert from 'node:assert/strict';
import test from 'node:test';
import { VideoCheckpoint } from '../media/video-checkpoint.ts';
import { deferred } from './store.fixtures.ts';
import type { MediaPlaybackBookmark } from './generated/MediaPlaybackBookmark';
const target = {
  scope: {
    expected_identity_id: 'owner',
    expected_device_id: 'phone',
    space_id: 'chat',
    message_id: 'video',
    item_id: 'file',
    version: 1,
    sha256: 'a'.repeat(64),
  },
  epoch: 1,
};
function fixture() {
  const player = {
    currentTime: 0,
    duration: 0,
    playing: false,
    pause() {
      this.playing = false;
    },
  };
  let ready = 0,
    current = true,
    value: MediaPlaybackBookmark = {
      revision: 'r1',
      position: { position_ms: 3500, duration_ms: 10000, rate_milli: 1000 },
    };
  const writes: any[] = [];
  const gate = deferred<MediaPlaybackBookmark>();
  let held = false;
  const session = new VideoCheckpoint(target, {
    player,
    current: () => current,
    ready: () => {
      ready++;
    },
    read: async () => (held ? gate.promise : value),
    save: async (scope, revision, position) => {
      assert.deepEqual(scope, { ...target.scope });
      assert.equal(revision, value.revision);
      writes.push(position);
      return (value = { revision: `r${writes.length + 1}`, position });
    },
  });
  return {
    session,
    player,
    writes,
    gate,
    hold: () => {
      held = true;
    },
    revoke: () => {
      current = false;
    },
    value: () => value,
    ready: () => ready,
  };
}
test('inline video restores only after native duration exists, stays paused and flushes the last paused point', async () => {
  const f = fixture();
  await f.session.open();
  assert.equal(f.ready(), 0);
  assert.equal(f.player.currentTime, 0);
  f.player.duration = 10;
  f.session.update();
  assert.equal(f.player.currentTime, 3.5);
  assert.equal(f.player.playing, false);
  assert.equal(f.ready(), 1);
  f.player.playing = true;
  f.player.currentTime = 7;
  f.session.update();
  f.player.currentTime = 7.5;
  f.session.pause();
  await f.session.flush();
  assert.equal(f.value().position?.position_ms, 7500);
  f.session.retire();
});
test('late inline restore after unmount never seeks or changes readiness', async () => {
  const f = fixture();
  f.hold();
  const opening = f.session.open();
  f.session.retire();
  f.player.duration = 10;
  f.gate.resolve({
    revision: 'old',
    position: { position_ms: 8000, duration_ms: 10000, rate_milli: 1000 },
  });
  await opening;
  assert.equal(f.player.currentTime, 0);
  assert.equal(f.ready(), 0);
  assert.deepEqual(f.writes, []);
});
test('a revoked inline context pauses before recording further playback', async () => {
  const f = fixture();
  f.player.duration = 10;
  await f.session.open();
  f.player.playing = true;
  f.revoke();
  f.player.currentTime = 8;
  f.session.update();
  await f.session.flush();
  assert.equal(f.player.playing, false);
  assert.equal(f.writes.length, 0);
});

test('a paused inline player adopts the gallery checkpoint on return without autoplay', async () => {
  const f = fixture();
  f.player.duration = 10;
  await f.session.open();
  f.player.currentTime = 8;
  f.session.pause();
  await f.session.flush();
  f.player.currentTime = 3.5;
  await f.session.reload();
  assert.equal(f.player.currentTime, 8);
  assert.equal(f.player.playing, false);
  f.session.retire();
});
