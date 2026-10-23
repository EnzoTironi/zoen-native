import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, tick } from './message-audio-session.fixtures.ts';

// Ownership tests exercise the real controller/coordinator; no React or permissive audio hook stub.
test('received playback and native lease survive row unmount, remount and second presentation', async () => {
  const f = fixture();
  const source = f.source();
  const first = f.session.channel(source);
  const unsubscribe = first.subscribe(() => {});
  await first.playFrom();
  await tick();
  const native = f.natives[0]!;
  native.seconds = 4.25;
  native.observe();
  first.setRate(1.5);
  unsubscribe();
  await tick();
  assert.equal(native.playing, true);
  assert.equal(native.disposed, false);
  assert.equal(f.released.length, 0);
  const second = f.session.channel(source);
  const third = f.session.channel(source);
  const offSecond = second.subscribe(() => {});
  const offThird = third.subscribe(() => {});
  assert.equal(second.getSnapshot().currentSeconds, 4.25);
  assert.equal(second.getSnapshot().rate, 1.5);
  second.pause();
  assert.equal(third.getSnapshot().playing, false);
  await third.playFrom();
  await tick();
  assert.equal(f.natives.length, 1);
  assert.equal(native.playing, true);
  offSecond();
  offThird();
  await f.session.dispose();
  assert.deepEqual(f.events.slice(-2), ['dispose:file:///owned-1', 'release:file:///owned-1']);
});

test('recording or another media intent cancels Play before materialization completes', async () => {
  const f = fixture();
  const gate = deferred<void>();
  f.materializeWait(gate.promise);
  const channel = f.session.channel(f.source());
  const playing = channel.playFrom();
  await tick();
  assert.equal(channel.getSnapshot().busy, true);
  f.coordinator.pauseAll();
  gate.resolve();
  await playing;
  await f.session.retireSession();
  assert.equal(f.natives.length, 0);
  assert.deepEqual(f.released, ['file:///owned-1']);
});

test('edited hash or version after download cannot be adopted as the reviewed voice source', async () => {
  for (const edit of ['hash', 'version', 'delete'] as const) {
    const f = fixture();
    const source = f.source();
    const gate = deferred<void>();
    f.materializeWait(gate.promise);
    const channel = f.session.channel(source);
    const playing = channel.playFrom();
    await tick();
    const record = f.records.get(source.messageId)!;
    if (edit === 'hash') record.hash = 'replacement';
    if (edit === 'version') record.version = 2;
    if (edit === 'delete') record.deleted = true;
    gate.resolve();
    await playing;
    await tick();
    assert.equal(f.natives.length, 0);
    assert.equal(channel.getSnapshot().loadFailed, true);
    assert.equal(f.released.length, 1);
    await f.session.dispose();
  }
});

test('canonical deletion while the row is absent stops and disposes received audio', async () => {
  const f = fixture();
  const source = f.source();
  const channel = f.session.channel(source);
  const off = channel.subscribe(() => {});
  await channel.playFrom();
  await tick();
  off();
  f.records.get(source.messageId)!.deleted = true;
  f.session.changed(['chat-A']);
  assert.equal(f.natives[0]!.playing, false, 'pause before awaiting a canonical reread');
  await tick();
  assert.equal(f.natives[0]!.disposed, true);
  assert.equal(f.released.length, 1);
  await f.session.dispose();
});

test('Pause while a coarse-event reread waits defeats conditional automatic resume', async () => {
  const f = fixture();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  const gate = deferred<void>();
  f.verifyWait(gate.promise);
  f.session.changed(['chat-A']);
  const plays = f.natives[0]!.plays;
  channel.pause();
  gate.resolve();
  await tick();
  await tick();
  assert.equal(f.natives[0]!.playing, false);
  assert.equal(f.natives[0]!.plays, plays);
  await f.session.dispose();
});

test('unrelated changes do not touch active playback; matching changes coalesce and preserve intent', async () => {
  const f = fixture();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  const verifies = f.events.filter((event) => event.startsWith('verify:')).length;
  f.session.changed(['chat-B']);
  assert.equal(f.natives[0]!.playing, true);
  assert.equal(f.events.filter((event) => event.startsWith('verify:')).length, verifies);
  const gate = deferred<void>();
  f.verifyWait(gate.promise);
  f.session.changed(['chat-A']);
  f.session.changed(['chat-A']);
  f.session.changed(['chat-A']);
  gate.resolve();
  await tick();
  await tick();
  assert.equal(f.natives[0]!.playing, true);
  assert.equal(f.natives.length, 1);
  assert.ok(f.events.filter((event) => event.startsWith('verify:')).length <= verifies + 3);
  await f.session.dispose();
});

test('native seek drains before player disposal and local-file release, then B can start', async () => {
  const f = fixture();
  const a = f.session.channel(f.source('A'));
  await a.playFrom();
  await tick();
  const gate = deferred<void>();
  f.seekWait(gate.promise);
  const seek = a.seekSeconds(5);
  await tick();
  const b = f.session.channel(f.source('B'));
  const playB = b.playFrom();
  await tick();
  assert.equal(f.natives.length, 1);
  assert.equal(f.natives[0]!.disposed, false);
  assert.equal(f.released.length, 0);
  f.seekWait(null);
  gate.resolve();
  await seek;
  await playB;
  await tick();
  assert.equal(f.natives[0]!.disposed, true);
  assert.equal(f.natives[1]!.playing, true);
  assert.ok(
    f.events.indexOf('dispose:file:///owned-1') < f.events.indexOf('release:file:///owned-1'),
  );
  assert.ok(
    f.events.indexOf('release:file:///owned-1') < f.events.indexOf('player:file:///owned-2'),
  );
  await f.session.dispose();
});

test('stale blur cannot cancel a newer focus; chat switch pauses and metadata resumes only by action', async () => {
  const f = fixture();
  const source = f.source();
  const channel = f.session.channel(source);
  await channel.playFrom();
  await tick();
  f.natives[0]!.seconds = 3;
  f.natives[0]!.observe();
  const sameChat = f.session.focus('chat-A');
  f.blur();
  assert.equal(f.natives[0]!.playing, true);
  const otherChat = f.session.focus('chat-B');
  assert.equal(f.natives[0]!.playing, false);
  await tick();
  otherChat();
  assert.equal(channel.getSnapshot().currentSeconds, 3);
  assert.equal(channel.getSnapshot().playing, false);
  await channel.playFrom();
  await tick();
  assert.equal(f.natives[1]!.seconds, 3);
  sameChat();
  await f.session.dispose();
});

test('background pauses and releases, foreground never reclaims an old play intent', async () => {
  const f = fixture();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  f.natives[0]!.seconds = 2;
  f.natives[0]!.observe();
  f.session.setForeground(false);
  assert.equal(f.natives[0]!.playing, false);
  await tick();
  assert.equal(f.natives[0]!.disposed, true);
  f.session.setForeground(true);
  assert.equal(channel.getSnapshot().playing, false);
  assert.equal(channel.getSnapshot().currentSeconds, 2);
  assert.equal(f.natives.length, 1);
  await f.session.dispose();
});

test('account change during materialization cannot publish or play an old account result', async () => {
  const f = fixture();
  const gate = deferred<void>();
  f.materializeWait(gate.promise);
  const channel = f.session.channel(f.source());
  const playing = channel.playFrom();
  await tick();
  f.setScope({ identityId: 'account-B', deviceId: 'phone-B', epoch: 2 });
  gate.resolve();
  await playing;
  await f.session.retireSession();
  assert.equal(f.natives.length, 0);
  assert.equal(channel.getSnapshot().loaded, false);
  assert.equal(f.released.length, 1);
});

test('membership revoke fences synchronously and session cleanup retains failed lease disposal for retry', async () => {
  const f = fixture();
  const channel = f.session.channel(f.source());
  await channel.playFrom();
  await tick();
  f.failReleases(1);
  f.revoke();
  assert.equal(f.natives[0]!.playing, false);
  await assert.rejects(f.session.retireSession(), /Lease cleanup failed/);
  assert.equal(f.natives[0]!.disposed, true);
  assert.equal(f.released.length, 0);
  await f.session.retireSession();
  assert.equal(f.released.length, 1);
});

test('metadata is bounded to 128 identities and is cleared at account-session retirement', async () => {
  const f = fixture();
  const firstSource = f.source('0');
  for (let index = 0; index < 129; ++index) {
    const channel = f.session.channel(index === 0 ? firstSource : f.source(String(index)));
    await channel.seekSeconds(2);
    channel.setRate(1.5);
  }
  assert.equal(f.session.channel(firstSource).getSnapshot().currentSeconds, 0);
  const last = f.session.channel(f.source('128'));
  assert.equal(last.getSnapshot().currentSeconds, 2);
  assert.equal(last.getSnapshot().rate, 1.5);
  assert.equal(f.natives.length, 0, 'metadata controls never allocate a native player');
  await f.session.retireSession();
  assert.equal(f.session.channel(last.source).getSnapshot().currentSeconds, 0);
  assert.equal(f.session.channel(last.source).getSnapshot().loaded, false);
});
