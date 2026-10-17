import assert from 'node:assert/strict';
import test from 'node:test';
import {
  clearDeviceNotificationState,
  type NotificationCleanupPort,
} from '../notifications/clear.ts';

const request = (identifier: string, device = 'device') => ({
  identifier,
  content: { data: { zoen_local: 1, device_id: device, space_id: 'space', event_seq: '1' } },
});

test('device notification cleanup awaits badge reset and removes only its presented and scheduled requests', async () => {
  const events: string[] = [];
  let finishBadge!: () => void;
  const port: NotificationCleanupPort = {
    presented: async () => [request('visible'), request('foreign-visible', 'other')],
    scheduled: async () => [request('pending'), request('foreign-pending', 'other')],
    dismiss: async (id) => {
      events.push(`dismiss:${id}`);
    },
    cancel: async (id) => {
      events.push(`cancel:${id}`);
    },
    clearBadge: () =>
      new Promise<void>((resolve) => {
        finishBadge = resolve;
      }),
    clearResponse: async () => {
      events.push('response');
    },
  };
  let done = false;
  const cleanup = clearDeviceNotificationState('device', port).then(() => {
    done = true;
  });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(done, false);
  assert.deepEqual(events.sort(), ['cancel:pending', 'dismiss:visible', 'response']);
  finishBadge();
  await cleanup;
  assert.equal(done, true);
});

test('one failed notification operation does not skip other removals, scheduled cancellation or badge cleanup', async () => {
  const events: string[] = [];
  await assert.rejects(
    clearDeviceNotificationState('device', {
      presented: async () => [request('first'), request('second')],
      scheduled: async () => [request('pending')],
      dismiss: async (id) => {
        events.push(id);
        if (id === 'first') throw Error('dismiss failed');
      },
      cancel: async (id) => {
        events.push(id);
      },
      clearBadge: async () => {
        events.push('badge');
      },
      clearResponse: async () => {
        events.push('response');
      },
    }),
    /notifications on this device could not be cleared/,
  );
  assert.deepEqual(events.sort(), ['badge', 'first', 'pending', 'response', 'second']);
});

test('a scheduled notification delivered during cancellation is included in the later presented cleanup', async () => {
  let delivered = false;
  const dismissed: string[] = [];
  await clearDeviceNotificationState('device', {
    scheduled: async () => [request('in-flight')],
    cancel: async () => {
      delivered = true;
    },
    presented: async () => (delivered ? [request('in-flight')] : []),
    dismiss: async (id) => {
      dismissed.push(id);
    },
    clearBadge: async () => {},
    clearResponse: async () => {},
  });
  assert.deepEqual(dismissed, ['in-flight']);
});
