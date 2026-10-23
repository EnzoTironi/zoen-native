import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { build } from 'esbuild';
import { backgroundPresentationJobs, type NativeNoteScope } from '../notifications/presentation.ts';
import type { LocalNote } from '../notifications/local.ts';
import type { Snapshot } from './types.ts';

const fixture = JSON.parse(
  readFileSync(
    new URL('../../modules/zoen-core/tests/fixtures/push-snapshots.json', import.meta.url),
    'utf8',
  ),
) as {
  before: Snapshot;
  after: Snapshot;
  expected: LocalNote[];
};

test('native presentation receives one scope-only wake while the JS local presenter stays unused', async () => {
  const scopes: NativeNoteScope[] = [],
    notes: LocalNote[] = [];
  const ports = {
    native: async (scope: NativeNoteScope) => {
      scopes.push(scope);
    },
    local: async (note: LocalNote) => {
      notes.push(note);
    },
  };
  await Promise.all(backgroundPresentationJobs(fixture.before, fixture.after, true, ports));
  assert.deepEqual(scopes, [
    { identityId: fixture.after.account!.identity_id, deviceId: fixture.after.account!.device_id },
  ]);
  assert.deepEqual(notes, []);
  assert.deepEqual(Object.keys(scopes[0]!).sort(), ['deviceId', 'identityId']);
  assert.equal(JSON.stringify(scopes).includes(fixture.expected[0]!.body), false);
  assert.deepEqual(backgroundPresentationJobs(fixture.after, fixture.after, true, ports), []);
});

test('Android and native-off presentation keep the existing per-note jobs and reject stale native scopes', async () => {
  const notes: LocalNote[] = [];
  const ports = {
    native: async () => {
      assert.fail('A disabled or stale native scope was submitted');
    },
    local: async (note: LocalNote) => {
      notes.push(note);
    },
  };
  const jobs = backgroundPresentationJobs(fixture.before, fixture.after, false, ports);
  assert.equal(jobs.length, 2);
  await Promise.all(jobs);
  assert.deepEqual(notes, fixture.expected);
  for (const account of [
    { ...fixture.after.account!, unlocked: false },
    { ...fixture.after.account!, setup_pending: true },
    { ...fixture.after.account!, identity_id: 'other-account' },
    { ...fixture.after.account!, device_id: 'other-device' },
  ]) {
    assert.deepEqual(
      backgroundPresentationJobs(fixture.before, { ...fixture.after, account }, true, ports),
      [],
    );
  }
});

test('actual SDK adapter borrows the existing module only for explicit iOS opt-in and the matching bridge version', async () => {
  const compiled = await build({
    entryPoints: [new URL('../notifications/system.ts', import.meta.url).pathname],
    bundle: true,
    write: false,
    format: 'cjs',
    platform: 'node',
    external: ['expo', 'expo-constants', 'expo-notifications', 'react-native'],
  });
  type Adapter = {
    nativeNotificationPresentation(): boolean;
    requestNativeNotes(scope: NativeNoteScope): Promise<void>;
  };
  const calls: unknown[][] = [];
  const bridge = {
    nativePushPresentationVersion: 1,
    notificationWake: async (...args: unknown[]) => {
      calls.push(args);
    },
  };
  function load(platform: string, enabled: boolean, native: unknown): Adapter {
    const module = { exports: {} };
    vm.runInNewContext(compiled.outputFiles[0]!.text, {
      module,
      exports: module.exports,
      require: (name: string) => {
        if (name === 'expo')
          return {
            requireOptionalNativeModule: (id: string) => {
              assert.equal(id, 'ZoenCore');
              return native;
            },
          };
        if (name === 'expo-constants')
          return {
            expoConfig: {
              extra: { zoenPush: { enabled, environment: 'sandbox', topic: 'xyz.tironi.zoen' } },
            },
          };
        if (name === 'react-native') return { Platform: { OS: platform } };
        if (name === 'expo-notifications') return {};
        throw new Error('Unexpected adapter dependency');
      },
    });
    return module.exports as Adapter;
  }
  const scope = { identityId: 'synthetic-owner', deviceId: 'synthetic-device' };
  const enabled = load('ios', true, bridge);
  assert.equal(enabled.nativeNotificationPresentation(), true);
  await enabled.requestNativeNotes(scope);
  assert.deepEqual(calls, [[scope.identityId, scope.deviceId]]);
  for (const adapter of [
    load('ios', false, bridge),
    load('android', true, bridge),
    load('ios', true, null),
    load('ios', true, { ...bridge, nativePushPresentationVersion: 0 }),
    load('ios', true, { nativePushPresentationVersion: 1 }),
  ]) {
    assert.equal(adapter.nativeNotificationPresentation(), false);
    await adapter.requestNativeNotes(scope);
  }
  assert.equal(calls.length, 1);
});
