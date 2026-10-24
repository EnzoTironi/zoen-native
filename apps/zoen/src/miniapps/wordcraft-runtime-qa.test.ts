import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { WORDCRAFT_ENGINE_REVISION } from './binary-formats.ts';
import {
  decodeWordcraftRuntimeQa,
  wordcraftRuntimeQaCommand,
  wordcraftRuntimeQaEnabled,
  wordcraftRuntimeQaOwnerReady,
  wordcraftRuntimeQaScript,
} from './wordcraft-runtime-qa.ts';
const token = '10000000-0000-4000-8000-000000000001';
function fixture(delayed = false, initiallyMounted = true) {
  const listeners = new Map<string, ((e: unknown) => void)[]>(),
    timers = new Map<number, () => unknown>(),
    sent: string[] = [],
    calls: string[] = [];
  let x = 24.5,
    mounted = initiallyMounted,
    mutation: (() => void) | null = null,
    disconnected = 0;
  let id = 0,
    release: () => void = () => {};
  const gate = delayed
    ? new Promise<void>((resolve) => {
        release = resolve;
      })
    : Promise.resolve();
  const parent = {},
    container = {},
    canvas = {
      tagName: 'CANVAS',
      width: 1206,
      height: 2622,
      parentElement: container,
      nextElementSibling: null as unknown,
      getBoundingClientRect: () => ({ x: 0, y: 0, width: 402, height: 874 }),
    };
  const input = {
    tagName: 'INPUT',
    type: 'text',
    previousElementSibling: canvas,
    parentElement: container,
    isConnected: true,
    getBoundingClientRect: () => ({ x, y: 152.5, width: 3, height: 3 }),
    get value() {
      throw Error('document content read');
    },
    focus() {
      assert.fail('diagnostic must not focus');
    },
  };
  canvas.nextElementSibling = input;
  const add = (kind: string, fn: (e: unknown) => void) =>
    listeners.set(kind, [...(listeners.get(kind) ?? []), fn]);
  const remove = (kind: string, fn: (e: unknown) => void) =>
    listeners.set(
      kind,
      (listeners.get(kind) ?? []).filter((f) => f !== fn),
    );
  const runtime = {
    load: async () => ({
      zoenHostRequest: async (envelope: string, bytes: Uint8Array) => {
        calls.push(envelope);
        assert.equal(bytes.length, 0);
        await gate;
        return {
          metadata: JSON.stringify({
            engineRevision: WORDCRAFT_ENGINE_REVISION,
            frozen: false,
            head: { version: '1', sha256: 'a'.repeat(64) },
            text: 'PRIVATE',
            scope: { actorId: 'SECRET' },
          }),
        };
      },
    }),
  };
  const window = {
    parent,
    __zoenRuntime: runtime,
    addEventListener: add,
    removeEventListener: remove,
    webkit: {
      messageHandlers: { ReactNativeWebView: { postMessage: (s: string) => sent.push(s) } },
    },
  };
  const document = {
    activeElement: input,
    hasFocus: () => true,
    visibilityState: 'visible',
    getElementById: (s: string) => (mounted && s === 'wordcraft_canvas' ? canvas : null),
    addEventListener: add,
    removeEventListener: remove,
  };
  runInNewContext(wordcraftRuntimeQaScript(token), {
    window,
    document,
    MutationObserver: class {
      constructor(callback: () => void) {
        mutation = callback;
      }
      observe() {}
      disconnect() {
        disconnected++;
        mutation = null;
      }
    },
    navigator: { userActivation: { isActive: true, hasBeenActive: true } },
    devicePixelRatio: 3,
    innerWidth: 402,
    innerHeight: 874,
    Uint8Array,
    getComputedStyle: () => ({ pointerEvents: 'auto' }),
    setTimeout: (f: () => unknown) => {
      timers.set(++id, f);
      return id;
    },
    clearTimeout: (n: number) => timers.delete(n),
  });
  const dispatch = (kind: string, event: unknown) => {
    for (const f of listeners.get(kind) ?? []) f(event);
  };
  const request = (patch = {}) =>
    dispatch('message', {
      source: parent,
      data: {
        marker: 'zoen.wordcraft.runtime.qa.v1',
        session: token,
        kind: 'snapshot',
        ownerReady: true,
        ...patch,
      },
    });
  const drain = async () => {
    for (let n = 0; n < 12; n++) {
      const queue = [...timers.values()];
      timers.clear();
      for (const f of queue) void f();
      await Promise.resolve();
    }
  };
  return {
    sent,
    calls,
    request,
    drain,
    release,
    dispatch,
    timers,
    move: (value: number) => {
      x = value;
    },
    mount: () => {
      mounted = true;
      mutation?.();
    },
    disconnected: () => disconnected,
  };
}
test('Word diagnostic is gated, fixed-command and only recognizes actual readiness notifications', () => {
  const runtime = {
    schema: 1,
    engine: 'wordcraft',
    abi: 'zoen-host-v1',
    revision: WORDCRAFT_ENGINE_REVISION,
  } as Parameters<typeof wordcraftRuntimeQaEnabled>[2];
  assert.equal(wordcraftRuntimeQaEnabled('ios', '1', runtime), true);
  for (const platform of ['android', 'web', 'macos'])
    assert.equal(wordcraftRuntimeQaEnabled(platform, '1', runtime), false);
  assert.equal(wordcraftRuntimeQaEnabled('ios', undefined, runtime), false);
  assert.equal(
    wordcraftRuntimeQaEnabled('ios', '1', { ...runtime!, revision: '0'.repeat(40) }),
    false,
  );
  assert.equal(
    wordcraftRuntimeQaOwnerReady(
      JSON.stringify({
        jsonrpc: '2.0',
        method: 'ui/update-model-context',
        params: { structuredContent: { zoen: { binarySave: { ready: true } } } },
      }),
    ),
    true,
  );
  assert.equal(wordcraftRuntimeQaOwnerReady('{"method":"other","params":{"ready":true}}'), null);
  assert.throws(() => wordcraftRuntimeQaCommand('injected()', true));
});
test('observer only reads status, captures activation at event time and never emits private data', async () => {
  const f = fixture();
  f.request({ session: 'wrong' });
  await f.drain();
  assert.deepEqual(f.calls, []);
  f.request();
  await f.drain();
  assert.deepEqual(f.calls, ['{"op":"status"}']);
  f.dispatch('focus', { type: 'focus', isTrusted: true });
  await f.drain();
  const value = decodeWordcraftRuntimeQa(f.sent.at(-1), token);
  assert.ok(value);
  assert.equal(value.engine, 1);
  assert.equal(value.frozen, 0);
  assert.equal(value.dom[4], 3);
  assert.deepEqual(value.activation.at(-1), [1, 6, 1, 1, 1]);
  assert.equal(f.sent.join().includes('PRIVATE'), false);
  assert.equal(f.sent.join().includes('SECRET'), false);
  assert.equal(decodeWordcraftRuntimeQa(f.sent[0], token.replace('1000', '2000')), null);
  const bad = JSON.parse(f.sent[0]!);
  bad.content = 'private';
  assert.equal(decodeWordcraftRuntimeQa(JSON.stringify(bad), token), null);
  for (const [index, value] of [
    [0, 8],
    [1, -1],
    [4, 0],
    [5, 9],
    [8, -1],
    [14, 1.5],
    [18, 2],
  ]) {
    const invalid = JSON.parse(f.sent[0]!);
    invalid.dom[index] = value;
    assert.equal(decodeWordcraftRuntimeQa(JSON.stringify(invalid), token), null);
  }

  f.dispatch('pagehide', {});
  f.request();
  await f.drain();
  assert.equal(f.timers.size, 0);
});
test('retirement suppresses a pending status result and subsequent callbacks', async () => {
  const f = fixture(true);
  f.request();
  await f.drain();
  assert.equal(f.calls.length, 1);
  f.dispatch('pagehide', {});
  f.release();
  await f.drain();
  assert.deepEqual(f.sent, []);
});

test('DOM and copied activation rows belong to the same post-status observation', async () => {
  const f = fixture(true);
  f.request();
  await f.drain();
  f.move(60);
  f.dispatch('focus', { type: 'focus', isTrusted: true });
  f.release();
  await f.drain();
  const value = decodeWordcraftRuntimeQa(f.sent[0], token);
  assert.ok(value);
  assert.equal(value.dom[6], 60);
  assert.equal(value.eventSerial, 1);
  assert.equal(value.activation.at(-1)?.[0], value.eventSerial);
});

test('readiness before mount samples the real canvas and cleans up its observer', async () => {
  const f = fixture(false, false);
  f.request();
  await f.drain();
  assert.equal(f.calls.length, 0);
  assert.equal(f.sent.length, 0);
  f.mount();
  await f.drain();
  assert.equal(f.calls.length, 1);
  assert.equal(f.disconnected(), 1);
  assert.equal(decodeWordcraftRuntimeQa(f.sent[0], token)?.engine, 1);
  const retired = fixture(false, false);
  retired.request();
  await retired.drain();
  retired.dispatch('pagehide', {});
  retired.mount();
  await retired.drain();
  assert.equal(retired.disconnected(), 1);
  assert.equal(retired.sent.length, 0);
  assert.equal(retired.calls.length, 0);
});
