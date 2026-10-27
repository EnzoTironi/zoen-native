import { createRequire } from 'node:module';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { ClientStore } from './store.ts';
import { Driver, deferred, draftSession } from './store.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { watchCallPresentation } from '../calls/presentation.ts';

const require = createRequire(import.meta.url);
const file = new URL('../calls/provider.ios.tsx', import.meta.url);
const transformed = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  configFile: false,
  babelrc: false,
  presets: ['babel-preset-expo'],
  caller: {
    name: 'metro',
    platform: 'ios',
    engine: 'hermes',
    isDev: false,
    supportsReactCompiler: true,
  },
}).code;
assert.match(transformed, /react\/compiler-runtime/, 'Exercise the production React compiler path');
const compiled = await build({
  stdin: { contents: transformed, loader: 'jsx' },
  bundle: false,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
});
type Calls = {
  available: boolean;
  activeCall: { call_id: string } | null;
  start(space: string, video: boolean): Promise<void>;
  showActiveCall(): Promise<void>;
};
async function fixture() {
  const driver = new Driver();
  driver.opening = async () => draftSession();
  const store = new ClientStore(driver);
  await store.open('account');
  const root = createReactTestRoot();
  const events = new Set<(event: { json: string }) => void>();
  const queries: ReturnType<typeof deferred<string>>[] = [];
  const actions: string[] = [];
  const showScopes: unknown[][] = [];
  const native = {
    startCall: async () => {
      actions.push('start');
    },
    callAudioReply: async () => {},
    showCurrentCall: async (...scope: unknown[]) => {
      showScopes.push(scope);
      actions.push('show');
    },
    callPresentation: () => {
      const query = deferred<string>();
      queries.push(query);
      return query.promise;
    },
    addListener: (name: string, callback: (event: { json: string }) => void) => {
      if (name === 'onCallState') events.add(callback);
      return {
        remove: () => {
          events.delete(callback);
        },
      };
    },
  };
  const module = {
    exports: {} as {
      NativeCallsProvider: React.ComponentType<React.PropsWithChildren>;
      useNativeCalls(): Calls;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/compiler-runtime' || name.startsWith('@babel/runtime/'))
        return require(name);
      if (name === 'react/jsx-runtime') return jsxRuntime;
      if (name === '../../modules/zoen-core/src/ZoenCoreModule')
        return { __esModule: true, default: native };
      if (name === '../core/provider')
        return {
          useClient: () => ({
            store,
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
          }),
        };
      if (name === './presentation.ts') return { watchCallPresentation };
      if (name === './audio-handoff.ts') return { createCallAudioHandoff: () => async () => {} };
      if (name === '../media/call-audio-focus.ts') return { callAudioFocus: {} };
      if (name === '../media/native-capture-owner.ts') return { nativeCaptureOwner: {} };
      if (name === '../media/playback-owner.ts') return { mediaPlayback: {} };
      if (name === 'expo-audio') return {};
      throw new Error(`Unexpected call port: ${name}`);
    },
  });
  let current!: Calls;
  function Consumer() {
    current = module.exports.useNativeCalls();
    return null;
  }
  await root.render(
    React.createElement(module.exports.NativeCallsProvider, null, React.createElement(Consumer)),
  );
  const account = store.getSnapshot().snapshot!.account!;
  const call = {
    identity_id: account.identity_id,
    device_id: account.device_id,
    space_id: 'read-chat',
    call_id: `call_${'a'.repeat(32)}`,
    state: 'connected',
    minimized: true,
    connected_at_ms: 1000,
  };
  return {
    root,
    store,
    driver,
    actions,
    showScopes,
    queries,
    call,
    current: () => current,
    event: async (value: unknown) =>
      root.act(() => {
        for (const callback of events) callback({ json: JSON.stringify(value) });
      }),
    close: async () => {
      await root.close();
      await store.close();
    },
  };
}

test('saved call actions cannot enter a reopened session with the same account and device', async () => {
  const f = await fixture();
  try {
    const original = f.current();
    const epoch = f.store.sessionEpoch;
    await f.root.act(() => f.store.open('account'));
    assert.ok(f.store.sessionEpoch > epoch);
    let staleErrors = 0;
    await original.start('read-chat', false).catch(() => {
      staleErrors++;
    });
    await original.showActiveCall();
    assert.equal(staleErrors, 0);
    assert.deepEqual(f.actions, []);
    await f.current().start('read-chat', false);
    assert.deepEqual(f.actions, ['start']);
  } finally {
    await f.close();
  }
});

test('minimized presentation belongs to its original session and cannot survive same-owner reopen', async () => {
  const f = await fixture();
  try {
    await f.event(f.call);
    const original = f.current();
    assert.equal(original.activeCall?.call_id, f.call.call_id);
    await original.showActiveCall();
    await original.start('read-chat', true);
    assert.deepEqual(f.actions, ['show', 'show']);
    assert.deepEqual(f.showScopes, [
      [f.call.identity_id, f.call.device_id, f.call.call_id],
      [f.call.identity_id, f.call.device_id, f.call.call_id],
    ]);
    await f.root.act(() => f.store.open('account'));
    assert.equal(f.current().activeCall, null);
    await original.showActiveCall();
    await original.start('read-chat', true);
    await f.root.act(() => f.queries[0]!.resolve(JSON.stringify(f.call)));
    assert.equal(f.current().activeCall, null);
    assert.deepEqual(f.actions, ['show', 'show']);
  } finally {
    await f.close();
  }
});

test('retirement fences call buttons before snapshot changes', async () => {
  const f = await fixture();
  const drained = deferred<void>();
  try {
    await f.event(f.call);
    const original = f.current();
    const snapshot = f.store.getSnapshot();
    f.store.registerSessionCleanup(() => drained.promise);
    await f.root.act(() => {
      void f.store.close();
    });
    assert.equal(f.store.getSnapshot(), snapshot);
    assert.equal(f.store.isSessionRetiring, true);
    await original.start('read-chat', false);
    await original.showActiveCall();
    assert.deepEqual(f.actions, []);
  } finally {
    drained.resolve();
    await f.close();
  }
});

test('unmounted call actions cannot initiate media in a surviving session', async () => {
  const f = await fixture();
  let unmounted = false;
  try {
    const original = f.current();
    await f.root.close();
    unmounted = true;
    await original.start('read-chat', false);
    assert.deepEqual(f.actions, []);
  } finally {
    if (!unmounted) await f.root.close();
    await f.store.close();
  }
});

test('locked account retires call UI and rejects saved actions without changing session identity', async () => {
  const f = await fixture();
  try {
    await f.event(f.call);
    const original = f.current();
    const epoch = f.store.sessionEpoch;
    const locked = draftSession();
    locked.account = { ...locked.account!, unlocked: false };
    f.driver.operation = async () => locked;
    await f.root.act(() => f.store.refresh());
    assert.equal(f.store.sessionEpoch, epoch);
    assert.equal(f.current().available, false);
    assert.equal(f.current().activeCall, null);
    await original.start('read-chat', false);
    await original.showActiveCall();
    assert.deepEqual(f.actions, []);
  } finally {
    await f.close();
  }
});
