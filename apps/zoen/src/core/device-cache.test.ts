import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import type { ClientStore } from './store.ts';

type Fixture = { calls: string[]; memory: boolean; disk: boolean };
const globals = globalThis as typeof globalThis & { __zoenImageCacheFixture?: Fixture };
const output = await build({
  stdin: {
    contents:
      "export { attachDeviceCacheCleanup } from '../media/device-cache'; export { sensitiveCacheWork } from '../media/cache-work';",
    resolveDir: new URL('.', import.meta.url).pathname,
  },
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'synthetic-image-cache',
      setup(plugin) {
        plugin.onResolve({ filter: /^expo-image$/ }, () => ({
          path: 'expo-image',
          namespace: 'fixture',
        }));
        plugin.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({
          contents: `export const Image = {
            async clearMemoryCache() { const f=globalThis.__zoenImageCacheFixture; f.calls.push('memory'); return f.memory; },
            async clearDiskCache() { const f=globalThis.__zoenImageCacheFixture; f.calls.push('disk'); return f.disk; },
          };`,
        }));
      },
    },
  ],
});
const cache = await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
);

function fixture() {
  const image = { calls: [] as string[], memory: true, disk: true };
  globals.__zoenImageCacheFixture = image;
  const state = { phase: 'ready', erasePending: false };
  let cleanup: (() => Promise<void>) | undefined;
  let changed: (() => void) | undefined;
  const store = {
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      changed = listener;
      return () => {
        changed = undefined;
      };
    },
    registerDeviceCleanup(listener: () => Promise<void>) {
      cleanup = listener;
      return () => {
        cleanup = undefined;
      };
    },
  } as unknown as ClientStore;
  const detach = cache.attachDeviceCacheCleanup(store);
  return { image, state, cleanup: () => cleanup!(), changed: () => changed?.(), detach };
}

test('iOS cache cleanup drains SDK file jobs before clearing both image caches', async () => {
  const previous = process.env.EXPO_OS;
  process.env.EXPO_OS = 'ios';
  const f = fixture();
  try {
    let finish!: () => void;
    const writer = cache.sensitiveCacheWork.run(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    f.state.phase = 'erasing';
    f.state.erasePending = true;
    f.changed();
    const cleanup = f.cleanup();
    assert.deepEqual(f.image.calls, []);
    await assert.rejects(
      cache.sensitiveCacheWork.run(async () => {}),
      /saindo/,
    );
    finish();
    await Promise.all([writer, cleanup]);
    assert.deepEqual(f.image.calls.sort(), ['disk', 'memory']);
    f.state.phase = 'onboarding';
    f.state.erasePending = false;
    f.changed();
    await cache.sensitiveCacheWork.run(async () => {});
  } finally {
    f.detach();
    if (previous === undefined) delete process.env.EXPO_OS;
    else process.env.EXPO_OS = previous;
  }
});

test('a false SDK cache-reset result fails closed and remains retryable', async () => {
  const previous = process.env.EXPO_OS;
  process.env.EXPO_OS = 'ios';
  const f = fixture();
  try {
    f.image.disk = false;
    f.state.phase = 'erasing';
    f.state.erasePending = true;
    await assert.rejects(f.cleanup(), /imagens/);
    assert.deepEqual(f.image.calls.sort(), ['disk', 'memory']);
    await assert.rejects(
      cache.sensitiveCacheWork.run(async () => {}),
      /saindo/,
    );
    f.image.disk = true;
    await f.cleanup();
    f.state.phase = 'onboarding';
    f.state.erasePending = false;
    f.changed();
    await cache.sensitiveCacheWork.run(async () => {});
  } finally {
    f.detach();
    if (previous === undefined) delete process.env.EXPO_OS;
    else process.env.EXPO_OS = previous;
  }
});

test('browser device cleanup does not call unsupported native image cache APIs', () => {
  const previous = process.env.EXPO_OS;
  process.env.EXPO_OS = 'web';
  let registered = false;
  try {
    cache.attachDeviceCacheCleanup({
      registerDeviceCleanup() {
        registered = true;
      },
    } as unknown as ClientStore)();
    assert.equal(registered, false);
  } finally {
    if (previous === undefined) delete process.env.EXPO_OS;
    else process.env.EXPO_OS = previous;
  }
});
