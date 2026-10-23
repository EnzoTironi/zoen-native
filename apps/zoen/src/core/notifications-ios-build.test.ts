import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import { build } from 'esbuild';
import {
  iosPushBuildConfiguration,
  notificationPluginOptions,
} from '../notifications/ios-build-config.ts';

const topic = 'xyz.tironi.zoen';
const apns = { enabled: true, environment: 'sandbox', topic } as const;

test('iOS build opt-in is explicit, scoped to this bundle and compatible with its signed environment', () => {
  assert.equal(
    iosPushBuildConfiguration(false, { private_key: 'unused' }, undefined, 'wrong'),
    null,
  );
  assert.deepEqual(iosPushBuildConfiguration(true, apns, topic, 'development'), apns);
  assert.deepEqual(
    iosPushBuildConfiguration(true, { ...apns, environment: 'production' }, topic, 'production'),
    { ...apns, environment: 'production' },
  );
  for (const [value, bundle, entitlement] of [
    [{}, topic, undefined],
    [{ ...apns, environment: 'development' }, topic, undefined],
    [{ platform: 'fcm', projectId: 'zoen-private' }, topic, undefined],
    [apns, 'other.app', undefined],
    [apns, undefined, undefined],
    [apns, topic, 'production'],
    [apns, topic, true],
  ] as const)
    assert.throws(() => iosPushBuildConfiguration(true, value, bundle, entitlement));
});

test('notifications options preserve presentation settings but cannot silently enable APNs', () => {
  const existing = { sounds: ['local.wav'], enableRemoteNotifications: true };
  assert.deepEqual(notificationPluginOptions(existing, null, undefined), {
    ...existing,
    enableRemoteNotifications: false,
    enableBackgroundRemoteNotifications: false,
  });
  assert.deepEqual(notificationPluginOptions(existing, apns, 'zoen-messages'), {
    ...existing,
    defaultChannel: 'zoen-messages',
    enableRemoteNotifications: true,
    enableBackgroundRemoteNotifications: true,
  });
});

test('actual CNG config enables iOS without Android and retires stale native wake flags when off', async () => {
  const compiled = await build({
    entryPoints: [new URL('../../app.config.ts', import.meta.url).pathname],
    bundle: true,
    write: false,
    format: 'cjs',
    platform: 'node',
  });
  // Read the actual source as part of this test's provenance, without loading native projects.
  assert.ok((await readFile(new URL('../../app.config.ts', import.meta.url), 'utf8')).length);
  type NativeConfig = {
    ios: { entitlements: Record<string, unknown>; infoPlist: Record<string, unknown> };
    plugins: (string | [string, Record<string, unknown>])[];
  };
  const module = { exports: {} as { default?: (context: unknown) => NativeConfig } };
  vm.runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'node:fs')
        return {
          readFileSync: () => {
            throw new Error('unexpected build file read');
          },
        };
      if (name === 'node:path') return { resolve: (...parts: string[]) => parts.join('/') };
      throw new Error('unexpected external build import');
    },
    process: { env: {} },
  });
  const configure = module.exports.default!;
  const base = { name: 'Synthetic', slug: 'synthetic', ios: { bundleIdentifier: topic } };
  const enabled = configure({
    config: { ...base, extra: { zoenPush: apns } },
    projectRoot: '/synthetic',
  });
  assert.equal(enabled.ios.entitlements['aps-environment'], 'development');
  assert.equal(enabled.ios.infoPlist.ZoenRemoteNotificationsEnabled, true);
  const notifications = enabled.plugins.find(
    (p) => Array.isArray(p) && p[0] === 'expo-notifications',
  );
  assert.deepEqual(JSON.parse(JSON.stringify(notifications)), [
    'expo-notifications',
    { enableRemoteNotifications: true, enableBackgroundRemoteNotifications: true },
  ]);
  const disabled = configure({ config: { ...base, ios: enabled.ios }, projectRoot: '/synthetic' });
  assert.equal(disabled.ios.infoPlist.ZoenRemoteNotificationsEnabled, false);
  assert.equal(disabled.ios.infoPlist.ZoenPushTopic, '');
  assert.throws(() =>
    configure({
      config: { ...base, plugins: ['expo-notifications', 'expo-notifications'] },
      projectRoot: '/synthetic',
    }),
  );
});
