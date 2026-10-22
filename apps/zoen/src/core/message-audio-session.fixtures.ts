import assert from 'node:assert/strict';
import { AudioClipController, type AudioClipPort } from '../media/audio-clip-controller.ts';
import {
  MessageAudioSession,
  type MessageAudioScope,
  type MessageAudioSource,
} from '../media/message-audio-session.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';
import type { LocalMediaFile } from '../media/types.ts';

export const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export function fixture() {
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
