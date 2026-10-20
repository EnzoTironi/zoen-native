import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AudioClipController, type AudioClipPort } from '../media/audio-clip-controller.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';
import {
  MessageAudioSession,
  type MessageAudioSource,
  type MessageAudioScope,
} from '../media/message-audio-session.ts';
import type { LocalMediaFile } from '../media/types.ts';

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture() {
  const coordinator = new PlaybackCoordinator();
  let scope: MessageAudioScope | null = { identityId: 'account-A', deviceId: 'phone-A', epoch: 1 };
  let authorized = true;
  let releaseFailures = 0;
  let verifyWait: Promise<void> | null = null;
  let materializeWait: Promise<void> | null = null;
  let seekWait: Promise<void> | null = null;
  let nextLoaded = true;
  let modeWait: Promise<void> | null = null;
  let failSetup = false;
  const records = new Map<string, { version: number; hash: string; deleted: boolean }>();
  const events: string[] = [];
  const released: string[] = [];
  const natives: Array<{
    seconds: number;
    playing: boolean;
    rate: number;
    plays: number;
    disposed: boolean;
    loaded: boolean;
    requested: boolean;
    controller: AudioClipController;
    observe(): void;
  }> = [];
  let files = 0;
  const session = new MessageAudioSession({
    coordinator,
    scope: () => scope,
    authorized: () => authorized,
    async verify(source) {
      events.push(`verify:${source.messageId}`);
      await verifyWait;
      const record = records.get(source.messageId);
      if (!record || record.deleted || record.hash !== source.sha256)
        throw new Error('Message revoked.');
      return record.version;
    },
    async materialize(source, version) {
      events.push(`materialize:${source.messageId}`);
      await materializeWait;
      return {
        uri: `file:///owned-${++files}`,
        itemId: source.itemId,
        version,
        info: {
          name: 'voice.m4a',
          path: '',
          mime: 'audio/mp4',
          bytes: '128',
          sha256: source.sha256,
          ready: true,
          chunks: 1,
          chunks_here: 1,
          has_thumbnail: false,
        },
      } satisfies LocalMediaFile;
    },
    async release(file) {
      if (releaseFailures-- > 0) throw new Error('Lease cleanup failed.');
      events.push(`release:${file.uri}`);
      released.push(file.uri);
    },
    create(source, file, current) {
      const controller = new AudioClipController(source.durationMs, coordinator);
      const native = {
        seconds: 0,
        playing: false,
        rate: 1,
        plays: 0,
        disposed: false,
        loaded: nextLoaded,
        requested: false,
        controller,
        observe() {
          controller.observe({
            isLoaded: native.loaded,
            playing: native.playing,
            isBuffering: false,
            currentTime: native.seconds,
            duration: 10,
            playbackRate: native.rate,
          });
        },
      };
      const pending = new Set<Promise<void>>();
      const alive = () => assert.equal(native.disposed, false, 'never use a disposed player');
      const port: AudioClipPort = {
        playing: () => {
          alive();
          return native.playing;
        },
        currentSeconds: () => {
          alive();
          return native.seconds;
        },
        play: () => {
          alive();
          native.playing = true;
          native.requested = true;
          ++native.plays;
        },
        pause: () => {
          alive();
          native.playing = false;
          native.requested = false;
        },
        seek(seconds) {
          alive();
          const work = (async () => {
            await seekWait;
            alive();
            native.seconds = seconds;
          })();
          pending.add(work);
          void work.finally(() => pending.delete(work));
          return work;
        },
        setRate: (rate) => {
          alive();
          native.rate = rate;
        },
        reload() {
          alive();
          native.observe();
        },
        preparePlayback: async () => {
          alive();
          const work = (async () => {
            await modeWait;
            alive();
          })();
          pending.add(work);
          void work.finally(() => pending.delete(work)).catch(() => {});
          await work;
        },
      };
      const disconnect = controller.connect(port);
      controller.configure(true, current, source.durationMs);
      native.observe();
      natives.push(native);
      events.push(`player:${file.uri}`);
      return {
        playback: controller,
        failure: failSetup ? new Error('Native listener setup failed.') : undefined,
        hasPlayRequest: () => native.requested,
        async dispose() {
          if (native.disposed) return;
          disconnect();
          while (pending.size) await Promise.allSettled(pending);
          native.disposed = true;
          events.push(`dispose:${file.uri}`);
        },
      };
    },
  });
  session.reconcile();
  const blur = session.focus('chat-A');
  function source(id = 'voice-A'): MessageAudioSource {
    records.set(id, { version: 1, hash: `hash-${id}`, deleted: false });
    return {
      scope: { ...scope! },
      spaceId: 'chat-A',
      messageId: id,
      itemId: `item-${id}`,
      sha256: `hash-${id}`,
      durationMs: 10_000,
    };
  }
  return {
    session,
    coordinator,
    records,
    source,
    natives,
    events,
    released,
    blur,
    verifyWait: (value: Promise<void> | null) => {
      verifyWait = value;
    },
    materializeWait: (value: Promise<void> | null) => {
      materializeWait = value;
    },
    seekWait: (value: Promise<void> | null) => {
      seekWait = value;
    },
    setLoaded: (value: boolean) => {
      nextLoaded = value;
    },
    modeWait: (value: Promise<void> | null) => {
      modeWait = value;
    },
    failSetup: () => {
      failSetup = true;
    },
    setScope(value: MessageAudioScope | null) {
      scope = value;
      session.reconcile();
    },
    revoke() {
      authorized = false;
      session.reconcile();
    },
    failReleases(count: number) {
      releaseFailures = count;
    },
  };
}

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
