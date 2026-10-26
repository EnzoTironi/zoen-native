import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PushCoordinator, type PushState } from '../notifications/coordinator.ts';
import {
  notificationSettingsPresentation as present,
  performNotificationAction,
} from '../notifications/settings-presentation.ts';
import { assertCatalogParity } from '../i18n/catalog.ts';
import { notifications } from '../i18n/catalogs/notifications.ts';

const state = (patch: Partial<PushState> = {}): PushState => ({
  permission: 'allowed',
  registration: 'acknowledged',
  busy: false,
  failure: null,
  deliveryVerified: false,
  ...patch,
});

test('permission controls take precedence over stale registration and never claim delivery', () => {
  for (const registration of [
    'unsupported',
    'unregistered',
    'registering',
    'acknowledged',
    'waiting',
    'refused',
  ] as const) {
    const denied = present(state({ registration, permission: 'denied' }), true);
    assert.equal(denied.title, 'disabledInSettings');
    assert.equal(denied.action, 'settings');
    const unchosen = present(state({ registration, permission: 'undetermined' }), true);
    assert.equal(unchosen.action, 'request');
    assert.equal(unchosen.title, 'notificationsOff');
    assert.equal(present(state({ registration, permission: 'unknown' }), true).action, 'refresh');
    assert.equal(present(state({ registration }), false).action, null);
  }
  const allowed = present(state(), true);
  assert.equal(allowed.title, 'allowedOnDevice');
  assert.equal(allowed.detail, null);
  assert.equal(allowed.action, 'settings');
  assert.equal(
    present(state({ registration: 'unsupported' }), true).detail,
    'remoteNotificationsAreNotYetAvailableOnThis',
  );
});

test('unconfirmed and failed registration offers retry while ongoing setup cannot be repeated', () => {
  for (const registration of ['waiting', 'refused'] as const) {
    const view = present(state({ registration }), true);
    assert.equal(view.title, 'setupPending');
    assert.equal(view.detail, 'permissionGranted');
    assert.equal(view.action, 'refresh');
    assert.equal(view.label, 'tryAgain');
    assert.equal(view.disabled, false);
  }
  assert.equal(present(state({ registration: 'unregistered' }), true).label, 'finishSetup');
  assert.equal(present(state({ failure: 'Token failed' }), true).action, 'refresh');
  for (const permission of ['unknown', 'undetermined', 'denied', 'allowed'] as const) {
    assert.equal(present(state({ permission, busy: true }), true).disabled, true);
    assert.equal(present(state({ permission, registration: 'registering' }), true).disabled, true);
  }
});

test('only explicit permission action prompts; retry uses the existing coordinator', async () => {
  const permissions: boolean[] = [];
  let registers = 0;
  let settings = 0;
  const coordinator = new PushCoordinator(
    {
      current: () => true,
      permission: async (request) => {
        permissions.push(request);
        return request ? 'allowed' : 'undetermined';
      },
      token: async () => 'aa',
      register: async () => {
        registers++;
        return { kind: 'acknowledged' };
      },
    },
    { topic: 'dev.zoen', environment: 'sandbox' },
  );
  const ports = {
    requestPermission: () => coordinator.foreground(true),
    refresh: () => coordinator.foreground(),
    openSettings: async () => {
      settings++;
    },
  };
  try {
    assert.deepEqual(permissions, []);
    await performNotificationAction('refresh', ports);
    assert.deepEqual(permissions, [false]);
    assert.equal(registers, 0);
    await performNotificationAction('request', ports);
    assert.deepEqual(permissions, [false, true]);
    assert.equal(registers, 1);
    assert.equal(coordinator.getSnapshot().registration, 'acknowledged');
    assert.equal(coordinator.getSnapshot().deliveryVerified, false);
    await performNotificationAction('settings', ports);
    await performNotificationAction(null, ports);
    assert.equal(settings, 1);
    assert.deepEqual(permissions, [false, true]);
    await assert.rejects(
      performNotificationAction('settings', {
        ...ports,
        openSettings: async () => {
          throw new Error('unavailable');
        },
      }),
      /unavailable/,
    );
  } finally {
    coordinator.dispose();
  }
  assertCatalogParity(notifications);
});
