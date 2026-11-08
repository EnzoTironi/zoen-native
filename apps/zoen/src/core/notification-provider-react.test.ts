import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as React from 'react';
import { createRequire } from 'node:module';
import { memberHookReactFixture } from './community-members-hook-react.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { PushCoordinator } from '../notifications/coordinator.ts';
import * as scopes from './account-scope.ts';
import * as links from '../notifications/links.ts';
import * as admission from '../notifications/local-admission.ts';
import * as platforms from '../notifications/native-adapter.ts';
import * as configuration from '../notifications/configuration.ts';
import * as presentation from '../notifications/presentation.ts';
import { notificationSystemFixture } from './notification-system.fixtures.ts';
import { NotificationJobs } from '../notifications/jobs.ts';
const require = createRequire(import.meta.url);

async function providerFixture(
  system: (
    f: Awaited<ReturnType<typeof memberHookReactFixture>>,
  ) => Record<string, unknown> = () => ({}),
) {
  const routes: unknown[] = [];
  const f = await memberHookReactFixture();
  const registrations: unknown[] = [];
  const operation = f.driver.operation;
  f.driver.operation = async (command) => {
    if (command.op === 'register_push') {
      registrations.push(structuredClone(command));
      return null;
    }
    return operation(command);
  };
  const dialogs = { inform: async () => {} };
  const Component = loadEditorAgent('../notifications/provider.tsx', {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      Platform: { OS: 'ios' },
      AppState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
    },
    'expo-router': {
      router: { push: (destination: unknown) => routes.push(structuredClone(destination)) },
    },
    '../i18n/index.ts': { t: (key: string) => key },
    '../core/provider': {
      useClient: () => ({
        store: f.store,
        ...React.useSyncExternalStore(f.store.subscribe, f.store.getSnapshot, f.store.getSnapshot),
      }),
    },
    '../core/account-scope': scopes,
    '../components/dialogs': { useDialogs: () => dialogs },
    './coordinator': { PushCoordinator },
    './links': links,
    './local-admission': admission,
    './native-adapter': platforms,
    './configuration': configuration,
    './presentation': presentation,
    './jobs': { NotificationJobs },
    './system': {
      nativePushConfiguration: () => ({ environment: 'sandbox', topic: 'xyz.tironi.zoen' }),
      notificationPermission: async () => 'allowed',
      nativePushToken: async () => 'ab12',
      listenPushToken: () => () => {},
      listenNotificationEvents: () => () => {},
      installNotificationHandler: () => () => {},
      clearDeviceNotifications: async () => {},
      updateBadge: async () => {},
      showLocalNote: async () => {},
      nativeNotificationPresentation: () => false,
      requestNativeNotes: async () => {},
      ...system(f),
    },
  }).NotificationProvider;
  return {
    ...f,
    registrations,
    routes,
    renderProvider: () => f.renderContent(React.createElement(Component)),
  };
}

test('optimized real provider registers after unlock without waiting for another foreground event', async () => {
  const f = await providerFixture();
  const { registrations } = f;
  try {
    f.snapshot.account!.unlocked = false;
    await f.act(() => f.store.refresh());
    await f.renderProvider();
    assert.equal(registrations.length, 0);
    f.snapshot.account!.unlocked = true;
    await f.act(() => f.store.refresh());
    assert.equal(registrations.length, 1);
    assert.deepEqual(registrations[0], {
      op: 'register_push',
      expected_identity_id: f.snapshot.account!.identity_id,
      expected_device_id: f.snapshot.account!.device_id,
      platform: 'apns-sandbox',
      token: 'ab12',
    });
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('actual optimized provider leaves a cold tap with the OS until unlock, handles once and does not replay after reopen', async () => {
  let pending: unknown = null;
  let cleared = 0;
  const f = await providerFixture((source) => {
    pending = {
      actionIdentifier: 'default',
      notification: {
        request: {
          identifier: 'cold-message',
          trigger: null,
          content: {
            data: {
              zoen_local: 1,
              device_id: source.snapshot.account!.device_id,
              space_id: source.snapshot.conversations[0]!.id,
              event_seq: '42',
              message_id: 'message-cold',
            },
          },
        },
      },
    };
    const adapter = notificationSystemFixture({
      DEFAULT_ACTION_IDENTIFIER: 'default',
      addNotificationReceivedListener: () => ({ remove() {} }),
      addNotificationResponseReceivedListener: () => ({ remove() {} }),
      getLastNotificationResponseAsync: async () => pending,
      clearLastNotificationResponseAsync: async () => {
        cleared++;
        pending = null;
      },
    });
    return { listenNotificationEvents: adapter.listenNotificationEvents };
  });
  try {
    f.snapshot.account!.unlocked = false;
    await f.act(() => f.store.refresh());
    await f.renderProvider();
    assert.equal(cleared, 0, 'locked account must not consume the OS response');
    assert.notEqual(pending, null);
    assert.deepEqual(f.routes, []);
    f.snapshot.account!.unlocked = true;
    await f.act(() => f.store.refresh());
    assert.deepEqual(f.routes, [
      {
        pathname: '/message/[space]/[message]',
        params: { space: f.snapshot.conversations[0]!.id, message: 'message-cold' },
      },
    ]);
    assert.equal(cleared, 1);
    assert.equal(pending, null);
    await f.reopen();
    assert.equal(f.routes.length, 1);
    assert.equal(cleared, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
