import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
const require = createRequire(import.meta.url),
  { transformSync } = require('@babel/core');
const compiled = await build({
  entryPoints: [new URL('../media/use-video-checkpoint.ts', import.meta.url).pathname],
  bundle: true,
  format: 'cjs',
  platform: 'node',
  write: false,
  external: ['react', 'react-native', '../core/provider'],
  plugins: [
    {
      name: 'production-hook',
      setup(builder) {
        builder.onLoad({ filter: /use-video-checkpoint\.ts$/ }, ({ path }) => ({
          loader: 'js',
          contents: transformSync(readFileSync(path, 'utf8'), {
            filename: path,
            babelrc: false,
            configFile: false,
            presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
            plugins: [['babel-plugin-react-compiler', { target: '19' }]],
            caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
          }).code,
        }));
      },
    },
  ],
});
async function fixture() {
  const root = createReactTestRoot(),
    listeners = new Map<string, () => void>(),
    cleanups = new Set<() => Promise<void>>(),
    device = new Set<() => Promise<void>>(),
    changes = new Set<(spaces?: string[]) => void>();
  let result: any,
    revision = 1,
    position = { position_ms: 3500, duration_ms: 10000, rate_milli: 1000 },
    deleted = false;
  const commands: any[] = [];
  const player = {
    duration: 10,
    currentTime: 0,
    playing: false,
    pause() {
      this.playing = false;
      listeners.get('playingChange')?.();
    },
    addListener(name: string, callback: () => void) {
      listeners.set(name, callback);
      return {
        remove() {
          listeners.delete(name);
        },
      };
    },
  };
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => ({
      phase: 'ready',
      mode: 'account',
      erasePending: false,
      snapshot: {
        account: { identity_id: 'me', device_id: 'phone', unlocked: true, setup_pending: false },
        me: { id: 'me' },
        conversations: [{ id: 'chat', members: [{ id: 'me' }] }],
      },
    }),
    subscribe: () => () => {},
    subscribeCanonicalChanges: (fn: (spaces?: string[]) => void) => {
      changes.add(fn);
      return () => changes.delete(fn);
    },
    registerSessionCleanup: (fn: () => Promise<void>) => {
      cleanups.add(fn);
      return () => cleanups.delete(fn);
    },
    registerDeviceCleanup: (fn: () => Promise<void>) => {
      device.add(fn);
      return () => device.delete(fn);
    },
    driver: {
      request: async (command: any) => {
        commands.push(command);
        if (command.op === 'message')
          return {
            id: 'video',
            deleted,
            kind: { Message: { card: { item_id: 'item', version: 1 } } },
          };
        if (command.op === 'media_playback_save') {
          assert.equal(command.expected_revision, `r${revision}`);
          position = command.position;
          revision++;
        }
        return { revision: `r${revision}`, position };
      },
    },
  };
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/compiler-runtime' || name.startsWith('@babel/runtime/'))
        return require(name);
      if (name === 'react-native')
        return { AppState: { addEventListener: () => ({ remove() {} }) } };
      if (name === '../core/provider') return { useClient: () => ({ store }) };
      throw Error(name);
    },
  });
  const scope = {
    expected_identity_id: 'me',
    expected_device_id: 'phone',
    space_id: 'chat',
    message_id: 'video',
    item_id: 'item',
    version: 1,
    sha256: 'a'.repeat(64),
  };
  function Screen() {
    result = module.exports.useVideoCheckpoint(player, scope, () => true);
    return null;
  }
  await root.render(React.createElement(Screen));
  await root.act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
  return {
    root,
    player,
    commands,
    cleanups,
    device,
    result: () => result,
    async remove() {
      deleted = true;
      await root.act(async () => {
        for (const changed of changes) changed(['chat']);
        for (let i = 0; i < 10; i++) await Promise.resolve();
      });
    },
  };
}
test('production-compiled inline hook restores quietly and device cleanup pauses, drains and withdraws playback', async () => {
  const f = await fixture();
  try {
    assert.equal(f.result().ready, true);
    assert.equal(f.player.currentTime, 3.5);
    assert.equal(f.player.playing, false);
    f.player.currentTime = 7.5;
    f.player.playing = true;
    await f.root.act(() => Promise.all([...f.device].map((cleanup) => cleanup())));
    assert.equal(f.player.playing, false);
    assert.equal(f.result().unavailable, true);
    assert.equal(
      f.commands.filter((command) => command.op === 'media_playback_save').at(-1).position
        .position_ms,
      7500,
    );
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
  assert.equal(f.cleanups.size, 0);
  assert.equal(f.device.size, 0);
});
test('canonical message deletion withdraws the actual mounted video controls without a React remount', async () => {
  const f = await fixture();
  try {
    f.player.playing = true;
    await f.remove();
    assert.equal(f.player.playing, false);
    assert.equal(f.result().unavailable, true);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});
