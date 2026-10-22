import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, tick } from './message-audio-session.fixtures.ts';

test('asynchronous first load preserves saved seek/rate through a broad canonical change', async () => {
  const f = fixture();
  const source = f.source();
  const channel = f.session.channel(source);
  await channel.seekSeconds(4.25);
  channel.setRate(1.5);
  f.setLoaded(false);
  await channel.playFrom();
  assert.equal(f.natives.length, 1);
  assert.equal(channel.getSnapshot().currentSeconds, 4.25);
  assert.equal(channel.getSnapshot().rate, 1.5);
  const gate = deferred<void>();
  f.verifyWait(gate.promise);
  f.session.changed([]); // Native empty arrays mean broad invalidation.
  assert.equal(f.natives[0]!.playing, false);
  f.natives[0]!.loaded = true;
  f.natives[0]!.observe();
  gate.resolve();
  await tick();
  await tick();
  assert.equal(f.natives[0]!.playing, true);
  assert.equal(f.natives[0]!.seconds, 4.25);
  assert.equal(f.natives[0]!.rate, 1.5);
  await f.session.dispose();
});

test('effect cleanup and immediate setup drain the old owner then allow the same account again', async () => {
  const f = fixture();
  const source = f.source();
  const channel = f.session.channel(source);
  await channel.playFrom();
  await tick();
  const cleanup = f.session.dispose();
  const setup = f.session.activate();
  const focus = f.session.focus('chat-A');
  await cleanup;
  await setup;
  assert.equal(f.natives[0]!.disposed, true);
  const newControls = f.session.channel(source);
  await newControls.playFrom();
  await tick();
  assert.equal(f.natives.length, 2);
  assert.equal(f.natives[1]!.playing, true);
  focus();
  await f.session.dispose();
});

test('Pause during initial-load canonical verification cancels preparation without a stuck spinner', async () => {
  const f = fixture();
  f.setLoaded(false);
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  const gate = deferred<void>();
  f.verifyWait(gate.promise);
  f.session.changed([]);
  assert.equal(channel.getSnapshot().busy, true);
  channel.pause();
  assert.equal(channel.getSnapshot().busy, false);
  gate.resolve();
  await tick();
  await tick();
  assert.equal(channel.getSnapshot().busy, false);
  assert.equal(f.natives[0]!.playing, false);
  assert.equal(f.natives[0]!.disposed, true);
  await f.session.dispose();
});

test('Pause or failure during initial native preparation preserves the previous resume position', async () => {
  for (const finish of ['pause', 'failure'] as const) {
    const f = fixture();
    const source = f.source();
    const channel = f.session.channel(source);
    await channel.seekSeconds(4.25);
    channel.setRate(1.5);
    const gate = deferred<void>();
    f.modeWait(gate.promise);
    await channel.playFrom();
    await tick();
    assert.equal(channel.getSnapshot().currentSeconds, 4.25);
    if (finish === 'pause') {
      channel.pause();
      gate.resolve();
    } else gate.reject(new Error('Native preparation failed.'));
    await tick();
    await tick();
    const remounted = f.session.channel(source);
    assert.equal(remounted.getSnapshot().currentSeconds, 4.25);
    assert.equal(remounted.getSnapshot().rate, 1.5);
    f.modeWait(null);
    await remounted.playFrom();
    await tick();
    assert.equal(f.natives[1]!.seconds, 4.25);
    await f.session.dispose();
  }
});

test('a surviving native Play request is preserved through revalidation while waiting for audio data', async () => {
  const f = fixture();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  f.natives[0]!.playing = false;
  f.natives[0]!.observe();
  assert.equal(f.natives[0]!.requested, true);
  f.session.changed(['chat-A']);
  await tick();
  await tick();
  assert.equal(f.natives[0]!.playing, true);
  assert.equal(f.natives.length, 1);
  await f.session.dispose();
});

test('a completed audio message does not replay after an incoming-message canonical refresh', async () => {
  const f = fixture();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  const native = f.natives[0]!;
  native.seconds = 10;
  native.playing = false;
  native.requested = false; // Runtime clears its accepted request on exact didJustFinish.
  native.observe();
  const plays = native.plays;
  f.session.changed([]);
  await tick();
  await tick();
  assert.equal(native.playing, false);
  assert.equal(native.plays, plays);
  await f.session.dispose();
});

test('failed native setup retains the runtime owner until disposal precedes lease release', async () => {
  const f = fixture();
  f.failSetup();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  assert.equal(channel.getSnapshot().loadFailed, true);
  assert.equal(channel.getSnapshot().busy, false);
  assert.deepEqual(f.events.slice(-2), ['dispose:file:///owned-1', 'release:file:///owned-1']);
  await f.session.dispose();
});

// RingRTC's notification-failure contract applies to our existing message audio
// owner too; this does not exercise or implement call signaling/native hardware.
for (const termination of ['background', 'revocation', 'retirement'] as const) {
  test(`a throwing playback observer cannot abandon audio/file teardown on ${termination}`, async (t) => {
    const f = fixture();
    const source = f.source();
    const channel = f.session.channel(source);
    const mirror = f.session.channel(source);
    let failNotification = false;
    let refusedNotifications = 0;
    let healthyNotifications = 0;
    const offBad = channel.subscribe(() => {
      if (!failNotification) return;
      refusedNotifications++;
      throw Error('Synthetic control notification failure');
    });
    const offHealthy = channel.subscribe(() => {
      healthyNotifications++;
    });
    const offMirror = mirror.subscribe(() => {});
    t.after(async () => {
      failNotification = false;
      offBad();
      offHealthy();
      offMirror();
      await f.session.dispose();
    });
    await channel.playFrom();
    await tick();
    const native = f.natives[0]!;
    assert.equal(native.playing, true);
    const healthyBefore = healthyNotifications;
    failNotification = true;
    if (termination === 'background') assert.doesNotThrow(() => f.session.setForeground(false));
    else if (termination === 'revocation') assert.doesNotThrow(() => f.revoke());
    else await f.session.retireSession();
    assert.equal(native.playing, false, 'Stop playback synchronously before asynchronous disposal');
    await f.session.retireSession();
    assert.ok(refusedNotifications > 0, 'The failing observer must actually be invoked');
    assert.ok(
      healthyNotifications > healthyBefore,
      'Other controls still receive the stopped state',
    );
    assert.equal(channel.getSnapshot().playing, false);
    assert.equal(mirror.getSnapshot().playing, false);
    assert.equal(native.disposed, true);
    assert.deepEqual(f.events.slice(-2), ['dispose:file:///owned-1', 'release:file:///owned-1']);
    assert.deepEqual(f.released, ['file:///owned-1']);
    await f.session.retireSession();
    assert.deepEqual(
      f.released,
      ['file:///owned-1'],
      'Repeated teardown cannot release another lease',
    );
  });
}
