import assert from 'node:assert/strict';
import { test } from 'node:test';
import { VoiceReviewOwner } from '../media/voice-review-owner.ts';
import { SensitiveCacheWork } from '../media/cache-work.ts';
import type {
  VoiceNativePort,
  VoicePreparedSource,
  VoiceSendable,
} from '../media/voice-session.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function prepared(id: string): VoicePreparedSource {
  return {
    id,
    uri: `file:///lease/${id}/source.m4a`,
    durationMs: 4000,
    sampleRate: 1000,
    frameCount: 4000,
    levels: [0.2, 0.8],
  };
}
function fixture(sharedWork = new SensitiveCacheWork(), uri = 'file:///cache/capture.m4a') {
  const events: string[] = [];
  const registrations = new Set<() => Promise<void>>();
  let sequence = 0,
    renders = 0;
  const port: VoiceNativePort = {
    prepare: async () => {
      const id = `source_${++sequence}`;
      events.push(`prepare:${id}`);
      return prepared(id);
    },
    render: async (id, ranges) => {
      events.push(`render:${id}`);
      const frameCount = ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0);
      return {
        uri: `file:///lease/${id}/render-${++renders}.m4a`,
        mime: 'audio/mp4',
        durationMs: frameCount,
        frameCount,
        levels: [0.2, 0.8],
      };
    },
    transcribe: async () => ({
      status: 'ready',
      text: 'Keep',
      words: [{ id: 'keep', text: 'Keep', startFrame: 0, endFrame: 1000 }],
    }),
    releaseRender: async (id) => {
      events.push(`release-render:${id}`);
    },
    cancel: async (id) => {
      events.push(`cancel:${id}`);
    },
    release: async (id) => {
      events.push(`release:${id}`);
    },
  };
  const options = {
    port,
    uri,
    work: sharedWork,
    registerCleanup: (callback: () => Promise<void>) => {
      registrations.add(callback);
      return () => {
        registrations.delete(callback);
        events.push('unregistered');
      };
    },
    discard: async (value: string) => {
      events.push(`discard:${value}`);
    },
  };
  const owner = new VoiceReviewOwner(options);
  return { owner, port, options, events, registrations, work: sharedWork, uri };
}
function capture(owner: VoiceReviewOwner): VoiceSendable {
  const value = owner.session.getSendable();
  assert.ok(value);
  return value;
}

test('close retains source and cleanup registration until a pending durable send is accepted', async () => {
  const f = fixture();
  await f.owner.prepare();
  const commit = deferred<string>();
  const started = deferred<void>();
  const sending = f.owner.send(capture(f.owner), async () => {
    f.events.push('commit');
    started.resolve();
    return commit.promise;
  });
  await started.promise;
  const closing = f.owner.close();
  assert.equal(f.registrations.size, 1);
  assert.equal(
    f.events.some((event) => event.startsWith('release:') || event.startsWith('discard:')),
    false,
  );
  commit.resolve('canonical-message');
  assert.equal(await sending, 'canonical-message');
  await closing;
  assert.equal(f.owner.session.getSnapshot().phase, 'closed');
  assert.equal(f.registrations.size, 0);
  assert.equal(f.events.filter((event) => event === `discard:${f.uri}`).length, 1);
});

test('explicit close also waits for a failed durable send before cleaning up', async () => {
  const f = fixture();
  await f.owner.prepare();
  const commit = deferred<void>();
  const started = deferred<void>();
  const sending = f.owner.send(capture(f.owner), async () => {
    started.resolve();
    return commit.promise;
  });
  const failure = assert.rejects(sending, /send failed/);
  await started.promise;
  const closing = f.owner.close();
  assert.equal(
    f.events.some((event) => event.startsWith('discard:')),
    false,
  );
  commit.reject(new Error('send failed'));
  await failure;
  await closing;
  assert.equal(f.registrations.size, 0);
  assert.equal(f.owner.session.getSnapshot().phase, 'closed');
});

test('failed send without close preserves the original, edit history and current preview for retry', async () => {
  const f = fixture();
  await f.owner.prepare();
  f.owner.session.edit({ type: 'cut', range: { startFrame: 1000, endFrame: 2000 } });
  await f.owner.session.retryPreview();
  const original = f.owner.session.getSnapshot().source;
  const sendable = capture(f.owner);
  await assert.rejects(
    f.owner.send(sendable, async () => {
      throw new Error('offline');
    }),
    /offline/,
  );
  assert.strictEqual(f.owner.session.getSnapshot().source, original);
  assert.equal(f.owner.session.getSnapshot().draft?.undo.length, 1);
  assert.strictEqual(capture(f.owner).preview, sendable.preview);
  assert.equal(
    f.events.some((event) => event.startsWith('discard:')),
    false,
  );
  assert.equal(f.registrations.size, 1);
  assert.equal(await f.owner.send(capture(f.owner), async () => 'accepted'), 'accepted');
  await f.owner.close();
});

test('the same accepted mutation cannot resend before or after a later cleanup failure', async () => {
  const f = fixture();
  await f.owner.prepare();
  const sent = capture(f.owner);
  let commits = 0;
  const commit = async () => {
    ++commits;
    return 'accepted-message';
  };
  assert.equal(await f.owner.send(sent, commit), 'accepted-message');
  await assert.rejects(f.owner.send(sent, commit));
  const release = f.port.release;
  f.port.release = async () => {
    throw new Error('cleanup busy');
  };
  await assert.rejects(f.owner.close());
  await assert.rejects(f.owner.send(sent, commit));
  assert.equal(commits, 1);
  assert.equal(f.registrations.size, 1);
  f.port.release = release;
  await f.owner.close();
  assert.equal(f.registrations.size, 0);
});

test('a synchronous commit callback can request close without releasing its in-flight audio', async () => {
  const f = fixture();
  await f.owner.prepare();
  const commit = deferred<string>();
  let closing: Promise<void> | undefined;
  const sending = f.owner.send(capture(f.owner), () => {
    closing = f.owner.close();
    return commit.promise;
  });
  assert.ok(closing);
  assert.equal(f.registrations.size, 1);
  assert.equal(
    f.events.some((event) => event.startsWith('release:') || event.startsWith('discard:')),
    false,
  );
  commit.resolve('committed');
  assert.equal(await sending, 'committed');
  await closing;
  assert.equal(f.owner.session.getSnapshot().phase, 'closed');
});

test('concurrent sends cannot invoke a second durable mutation', async () => {
  const f = fixture();
  await f.owner.prepare();
  const pending = deferred<string>();
  const captured = capture(f.owner);
  let commits = 0;
  const first = f.owner.send(captured, () => {
    ++commits;
    return pending.promise;
  });
  await assert.rejects(
    f.owner.send(captured, async () => {
      ++commits;
      return 'duplicate';
    }),
  );
  assert.equal(commits, 1);
  pending.resolve('first');
  assert.equal(await first, 'first');
  await f.owner.close();
});

test('a capture invalidated by a new edit cannot reach the durable commit', async () => {
  const f = fixture();
  await f.owner.prepare();
  const stale = capture(f.owner);
  f.owner.session.edit({ type: 'cut', range: { startFrame: 1000, endFrame: 2000 } });
  await f.owner.session.retryPreview();
  let commits = 0;
  await assert.rejects(
    f.owner.send(stale, async () => {
      ++commits;
    }),
  );
  assert.equal(commits, 0);
  assert.equal(capture(f.owner).preview.frameCount, 3000);
  await f.owner.close();
});

test('a stale transcript or foreign preview cannot be attached to the current audio send', async () => {
  const f = fixture();
  await f.owner.prepare();
  const beforeSpeech = capture(f.owner);
  await f.owner.session.transcribe('en-US');
  let commits = 0;
  const commit = async () => {
    ++commits;
  };
  await assert.rejects(f.owner.send(beforeSpeech, commit));
  const current = capture(f.owner);
  await assert.rejects(
    f.owner.send(
      { ...current, preview: { ...current.preview, uri: 'file:///foreign.m4a' } },
      commit,
    ),
  );
  await assert.rejects(f.owner.send({ ...current, transcript: 'forged searchable text' }, commit));
  assert.equal(commits, 0);
  await f.owner.close();
});

test('cleanup registration survives native-release failure and remains callable for retry', async () => {
  const f = fixture();
  await f.owner.prepare();
  const cleanup = [...f.registrations][0]!;
  let fail = true;
  f.port.release = async (id) => {
    f.events.push(`release:${id}`);
    if (fail) throw new Error('release failed');
  };
  await assert.rejects(f.owner.close());
  assert.equal(f.registrations.has(cleanup), true);
  fail = false;
  await cleanup();
  assert.equal(f.registrations.size, 0);
  assert.equal(f.events.filter((event) => event === 'unregistered').length, 1);
});

test('failed original-file discard keeps cleanup ownership after the native session has closed', async () => {
  const f = fixture();
  await f.owner.close();
  let attempts = 0;
  f.options.uri = 'file:///cache/discard-retry.m4a';
  f.options.discard = async (uri) => {
    f.events.push(`discard:${uri}`);
    if (++attempts === 1) throw new Error('cache busy');
  };
  const owner = new VoiceReviewOwner(f.options);
  await owner.prepare();
  await assert.rejects(owner.close(), /cache busy/);
  assert.equal(owner.session.getSnapshot().phase, 'closed');
  assert.equal(f.registrations.size, 1);
  await [...f.registrations][0]!();
  assert.equal(f.registrations.size, 0);
  assert.equal(attempts, 2);
});

test('close during prepare waits for the late native copy before discarding the original', async () => {
  const f = fixture();
  const copy = deferred<VoicePreparedSource>();
  f.port.prepare = () => copy.promise;
  const preparing = f.owner.prepare();
  const closing = f.owner.close();
  assert.equal(f.registrations.size, 1);
  assert.equal(
    f.events.some((event) => event.startsWith('discard:')),
    false,
  );
  copy.resolve(prepared('late_source'));
  await Promise.allSettled([preparing]);
  await closing;
  assert.ok(f.events.includes('release:late_source'));
  assert.equal(f.registrations.size, 0);
});

test('a closed owner cannot admit another prepare or send even after cache work resumes', async () => {
  const f = fixture();
  await f.owner.prepare();
  const before = capture(f.owner);
  await f.owner.close();
  f.work.resume();
  let commits = 0;
  await assert.rejects(f.owner.prepare());
  await assert.rejects(
    f.owner.send(before, async () => {
      ++commits;
    }),
  );
  assert.equal(commits, 0);
  assert.equal(f.events.filter((event) => event.startsWith('prepare:')).length, 1);
  await f.owner.close();
  assert.equal(f.events.filter((event) => event === `discard:${f.uri}`).length, 1);
});

test('cache erasure refuses new voice work but still admits retained owner cleanup', async () => {
  const f = fixture();
  await f.owner.prepare();
  const sendable = capture(f.owner);
  await f.work.erase(async () => {
    f.events.push('cache-cleared');
  });
  let commits = 0;
  await assert.rejects(
    f.owner.send(sendable, async () => {
      ++commits;
    }),
  );
  assert.equal(commits, 0);
  await f.owner.close();
  assert.equal(f.registrations.size, 0);
});

test('a new owner cannot prepare a native copy after the shared cache admission gate is paused', async () => {
  const work = new SensitiveCacheWork();
  await work.erase(async () => {});
  const f = fixture(work, 'file:///cache/fenced-capture.m4a');
  await assert.rejects(f.owner.prepare());
  assert.equal(
    f.events.some((event) => event.startsWith('prepare:')),
    false,
  );
  await f.owner.close();
  assert.equal(f.registrations.size, 0);
});

test('parent-retained owners for distinct captures stay independent across repeated child cleanup', async () => {
  const f = fixture();
  const copyA = deferred<VoicePreparedSource>();
  const copyB = deferred<VoicePreparedSource>();
  let copies = 0;
  f.port.prepare = () => (++copies === 1 ? copyA.promise : copyB.promise);
  const preparingA = f.owner.prepare();
  const closingA = f.owner.close();
  assert.strictEqual(f.owner.close(), closingA);
  const uriB = 'file:///cache/second-capture.m4a';
  const ownerB = new VoiceReviewOwner({ ...f.options, uri: uriB });
  const preparingB = ownerB.prepare();
  copyA.resolve(prepared('capture_a'));
  await Promise.allSettled([preparingA]);
  await closingA;
  assert.ok(f.events.includes('release:capture_a'));
  assert.equal(f.events.filter((event) => event === `discard:${f.uri}`).length, 1);
  assert.equal(f.events.includes(`discard:${uriB}`), false);
  assert.equal(f.registrations.size, 1);
  copyB.resolve(prepared('capture_b'));
  await preparingB;
  assert.equal(ownerB.session.getSnapshot().source?.id, 'capture_b');
  assert.equal(capture(ownerB).preview.frameCount, 4000);
  await ownerB.close();
  await ownerB.close();
  assert.equal(f.registrations.size, 0);
  assert.equal(f.events.filter((event) => event === `discard:${uriB}`).length, 1);
});

test('global erasure waits for a pending durable send and its retained cleanup before clearing caches', async () => {
  const f = fixture();
  await f.owner.prepare();
  const commit = deferred<void>();
  const started = deferred<void>();
  const release = deferred<void>();
  const releaseStarted = deferred<void>();
  f.port.release = async () => {
    releaseStarted.resolve();
    await release.promise;
    f.events.push('lease-released');
  };
  const sending = f.owner.send(capture(f.owner), async () => {
    started.resolve();
    return commit.promise;
  });
  await started.promise;
  const erase = f.work.erase(async () => {
    f.events.push('cache-cleared');
  });
  const cleanup = [...f.registrations][0]!();
  commit.resolve();
  await sending;
  await releaseStarted.promise;
  assert.equal(f.events.includes('cache-cleared'), false);
  assert.equal(f.registrations.size, 1);
  release.resolve();
  await Promise.all([cleanup, erase]);
  assert.ok(f.events.indexOf('lease-released') < f.events.indexOf('cache-cleared'));
  assert.ok(f.events.indexOf(`discard:${f.uri}`) < f.events.indexOf('cache-cleared'));
});
