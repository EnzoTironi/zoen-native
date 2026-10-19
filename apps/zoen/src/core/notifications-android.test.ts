import test from 'node:test';
import assert from 'node:assert/strict';
import { PushCoordinator, type PushPorts } from '../notifications/coordinator.ts';
import { androidFcmBuildConfiguration } from '../notifications/build-config.ts';
import {
  apnsToken,
  fcmToken,
  pushConfiguration,
  pushPlatform,
} from '../notifications/configuration.ts';
import {
  decodeNotificationWake,
  nativePushConfigurationFor,
  nativeTokenFromWire,
  readNativePermission,
  readNativeToken,
} from '../notifications/native-adapter.ts';
import { decodeWake } from '../notifications/wake.ts';

const fcm = { platform: 'fcm', projectId: 'zoen-private' } as const;
const apns = { environment: 'sandbox', topic: 'xyz.tironi.zoen' } as const;
const token = 'CaseSensitive:Registration_01-ABC';
const firebase = () => ({
  project_info: { project_id: fcm.projectId, project_number: '123456789' },
  client: [{ client_info: { android_client_info: { package_name: 'xyz.tironi.zoen' } } }],
});

test('FCM uses an independent explicit Android build opt-in without changing APNs configuration', () => {
  assert.deepEqual(pushConfiguration(fcm), fcm);
  assert.equal(pushPlatform(fcm), 'fcm');
  assert.equal(pushPlatform(apns), 'apns-sandbox');
  assert.equal(pushPlatform({ ...apns, environment: 'production' }), 'apns');
  const extra = { enabled: true, ...apns, android: { enabled: true, ...fcm } };
  assert.deepEqual(nativePushConfigurationFor('android', extra), fcm);
  assert.deepEqual(nativePushConfigurationFor('ios', extra), apns);
  for (const value of [
    apns,
    { enabled: true, ...apns },
    { enabled: true, ...fcm },
    { android: fcm },
    { android: { enabled: false, ...fcm } },
  ])
    assert.equal(nativePushConfigurationFor('android', value), null);
  assert.equal(nativePushConfigurationFor('ios', { enabled: true, ...fcm }), null);
  assert.equal(nativePushConfigurationFor('web', extra), null);
  for (const projectId of ['', '../zoen', 'ZOEN', 'x', 'a'.repeat(31)])
    assert.equal(pushConfiguration({ ...fcm, projectId }), null);
});

test('FCM token boundaries match the native relay without APNs hex normalization', () => {
  assert.equal(fcmToken(token), token);
  assert.equal(fcmToken('A'.repeat(4096))?.length, 4096);
  assert.equal(fcmToken('AbCd'), 'AbCd');
  assert.equal(apnsToken('AbCd'), 'abcd');
  for (const value of ['', 'A'.repeat(4097), 'a/b', 'a.b', 'a=b', ' a', 'a\n', 'á', {}, 1])
    assert.equal(fcmToken(value), null);
  assert.equal(nativeTokenFromWire('android', { type: 'android', data: token }), token);
  for (const value of [
    { type: 'ios', data: token },
    { type: 'expo', data: 'ExponentPushToken[x]' },
    { type: 'android', data: 1 },
    null,
  ])
    assert.equal(nativeTokenFromWire('android', value), null);
});

test('Firebase build configuration is inert until enabled and checks the actual app package', () => {
  assert.equal(androidFcmBuildConfiguration(false, { private_key: 'never-used' }, undefined), null);
  assert.deepEqual(androidFcmBuildConfiguration(true, firebase(), 'xyz.tironi.zoen'), {
    enabled: true,
    ...fcm,
  });
  for (const value of [
    null,
    {},
    { ...firebase(), client: [] },
    { ...firebase(), private_key: 'not-a-client-config' },
    { ...firebase(), project_info: { project_id: '../wrong' } },
  ])
    assert.throws(() => androidFcmBuildConfiguration(true, value, 'xyz.tironi.zoen'));
  assert.throws(() => androidFcmBuildConfiguration(true, firebase(), 'another.app'));
  assert.throws(() => androidFcmBuildConfiguration(true, firebase(), undefined));
});

test('passive permission reads never create a channel or prompt; Android explicit prompt waits for its channel', async () => {
  const order: string[] = [];
  let complete: () => void = () => {};
  const port = {
    channel: () =>
      new Promise<void>((resolve) => {
        order.push('channel');
        complete = resolve;
      }),
    read: async () => {
      order.push('read');
      return 'undetermined';
    },
    prompt: async () => {
      order.push('prompt');
      return 'allowed';
    },
  };
  assert.equal(await readNativePermission('android', false, port), 'undetermined');
  assert.deepEqual(order, ['read']);
  const pending = readNativePermission('android', true, port);
  assert.deepEqual(order, ['read', 'channel']);
  complete();
  assert.equal(await pending, 'allowed');
  assert.deepEqual(order, ['read', 'channel', 'prompt']);
  await readNativePermission('ios', true, port);
  assert.equal(order.at(-1), 'prompt');
});

test('Android obtains its native FCM token only after the channel succeeds', async () => {
  const order: string[] = [];
  const port = {
    channel: async () => {
      order.push('channel');
    },
    token: async () => {
      order.push('token');
      return { type: 'android', data: token };
    },
  };
  assert.equal(await readNativeToken('android', port), token);
  assert.deepEqual(order, ['channel', 'token']);
  order.length = 0;
  await assert.rejects(
    readNativeToken('android', {
      ...port,
      channel: async () => {
        throw new Error('channel failed');
      },
    }),
  );
  assert.equal(order.length, 0);
  await assert.rejects(
    readNativePermission('android', true, {
      channel: async () => {
        throw new Error('channel failed');
      },
      read: async () => {},
      prompt: async () => {
        order.push('prompt');
      },
    }),
  );
  assert.equal(order.length, 0);
  assert.equal(
    await readNativeToken('ios', { ...port, token: async () => ({ type: 'ios', data: 'ABCD' }) }),
    'ABCD',
  );
  assert.equal(order.length, 0);
});

test('actual relay FCM wake and Expo58 serialized RemoteMessage are handled without APNs assumptions', () => {
  const data = { wake: '1' };
  const hint = { spaceId: null, eventSequence: null };
  assert.deepEqual(decodeWake(data, 'fcm'), hint);
  assert.deepEqual(decodeWake(JSON.stringify(data), 'fcm'), hint);
  assert.equal(decodeWake(data), null);
  for (const serialized of [data, { ...data, dataString: null }])
    assert.deepEqual(
      decodeNotificationWake('android', {
        type: 'push',
        remoteMessage: { data: serialized, notification: null, messageId: 'transport-metadata' },
      }),
      hint,
    );
  const ios = { aps: { 'content-available': 1 } };
  assert.deepEqual(decodeNotificationWake('ios', { type: 'push', payload: ios }), hint);
  assert.equal(decodeWake(ios, 'fcm'), null);
  assert.equal(decodeNotificationWake('android', { type: 'push', payload: ios }), null);
  assert.equal(
    decodeNotificationWake('ios', { type: 'push', remoteMessage: { data, notification: null } }),
    null,
  );
});

test('FCM content, actions, navigation, coercions and fake local notifications cannot become wake instructions', () => {
  for (const data of [
    { wake: true },
    { wake: 1 },
    { wake: '01' },
    { wake: '1', url: 'zoen://chat/any' },
    { wake: '1', title: 'remote content' },
    { wake: '1', approve: 'yes' },
    { wake: '1', dataString: '{}' },
    { wake: '1', body: '{}' },
  ])
    assert.equal(
      decodeNotificationWake('android', {
        type: 'push',
        remoteMessage: { data, notification: null },
      }),
      null,
    );
  assert.equal(
    decodeNotificationWake('android', {
      type: 'push',
      remoteMessage: { data: { wake: '1' }, notification: { title: 'remote' } },
    }),
    null,
  );
  assert.equal(
    decodeNotificationWake('android', {
      type: 'timeInterval',
      remoteMessage: { data: { wake: '1' }, notification: null },
    }),
    null,
  );
  assert.equal(decodeWake('x'.repeat(4097), 'fcm'), null);
});

test('Android registration requests permission only from the explicit action and uses the authenticated fcm operation', async () => {
  const order: string[] = [];
  const coordinator = new PushCoordinator(
    {
      current: () => true,
      permission: (request) =>
        readNativePermission('android', request, {
          channel: async () => {
            order.push('channel');
          },
          read: async () => {
            order.push('permission:read');
            return 'undetermined' as const;
          },
          prompt: async () => {
            order.push('permission:prompt');
            return 'allowed' as const;
          },
        }),
      token: () =>
        readNativeToken('android', {
          channel: async () => {
            order.push('channel');
          },
          token: async () => {
            order.push('token');
            return { type: 'android', data: token };
          },
        }),
      register: async (config, value) => {
        order.push(`register:${pushPlatform(config)}:${value}`);
        return { kind: 'acknowledged' };
      },
    },
    fcm,
  );
  await coordinator.foreground();
  assert.deepEqual(order, ['permission:read']);
  await coordinator.foreground(true);
  assert.deepEqual(order, [
    'permission:read',
    'channel',
    'permission:prompt',
    'channel',
    'token',
    `register:fcm:${token}`,
  ]);
  assert.equal(coordinator.getSnapshot().registration, 'acknowledged');
  assert.equal(coordinator.getSnapshot().deliveryVerified, false);
  coordinator.dispose();
});

for (const config of [fcm, apns]) {
  test(`${pushPlatform(config)} native rotation wins over a stale in-flight token query`, async () => {
    const writes: string[] = [];
    let finish: (value: string) => void = () => {};
    const old = config === fcm ? 'Old:Token_ABC' : 'AA'.repeat(32);
    const fresh = config === fcm ? 'New:Token_XYZ' : 'BB'.repeat(32);
    const coordinator = new PushCoordinator(
      {
        current: () => true,
        permission: async () => 'allowed',
        token: () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
        register: async (_, value) => {
          writes.push(value);
          return { kind: 'acknowledged' };
        },
      },
      config,
    );
    const foreground = coordinator.foreground();
    await Promise.resolve();
    await coordinator.receivedToken(fresh);
    finish(old);
    await foreground;
    assert.deepEqual(writes, [config === fcm ? fresh : fresh.toLowerCase()]);
    coordinator.dispose();
  });
}

test('FCM rotations remain serialized, denied permission removes the token, and late account callbacks cannot register', async () => {
  const writes: string[] = [];
  let current = true;
  let finish: () => void = () => {};
  const ports: PushPorts = {
    current: () => current,
    permission: async () => 'allowed',
    token: async () => token,
    register: async (_, value) => {
      writes.push(value);
      if (writes.length === 1)
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
      return { kind: 'acknowledged' };
    },
  };
  const coordinator = new PushCoordinator(ports, fcm);
  const first = coordinator.foreground();
  await Promise.resolve();
  await Promise.resolve();
  const rotate = coordinator.receivedToken('Rotated:Token_XYZ');
  assert.deepEqual(writes, [token]);
  finish();
  await Promise.all([first, rotate]);
  assert.deepEqual(writes, [token, 'Rotated:Token_XYZ']);
  ports.permission = async () => 'denied';
  await coordinator.foreground();
  assert.equal(writes.at(-1), '');
  ports.permission = async () => 'allowed';
  ports.token = async () => {
    current = false;
    return 'WrongAccount:Token';
  };
  await coordinator.foreground();
  assert.equal(writes.length, 3);
  coordinator.dispose();
});
