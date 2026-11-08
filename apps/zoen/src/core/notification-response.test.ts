import assert from 'node:assert/strict';
import test from 'node:test';
import { notificationSystemFixture } from './notification-system.fixtures.ts';
import { deferred } from './store.fixtures.ts';

for (const initialFirst of [false, true]) {
  test(`one local tap is handled once (initial response first: ${initialFirst})`, async () => {
    const response = {
      actionIdentifier: 'default',
      notification: { request: { identifier: 'message-1', trigger: null, content: { data: {} } } },
    };
    let receive: ((value: typeof response) => void) | undefined;
    let cleared = 0;
    const notifications = {
      DEFAULT_ACTION_IDENTIFIER: 'default',
      addNotificationReceivedListener: () => ({ remove() {} }),
      addNotificationResponseReceivedListener: (callback: typeof receive) => {
        receive = callback;
        return { remove() {} };
      },
      getLastNotificationResponseAsync: async () => response,
      clearLastNotificationResponseAsync: async () => {
        cleared += 1;
      },
    };
    const system = notificationSystemFixture(notifications);
    let taps = 0;
    const stop = system.listenNotificationEvents(
      () => {},
      () => {
        taps += 1;
      },
    );
    assert.ok(receive);
    if (initialFirst) await Promise.resolve();
    receive(response);
    await Promise.resolve();
    assert.equal(taps, 1);
    assert.equal(cleared, 1);
    receive(response);
    assert.equal(taps, 2);
    stop();
  });
}

for (const refused of ['deferred', 'throws'] as const)
  test(`an unhandled ${refused} response stays in the native SDK for a replacement listener`, async () => {
    const response = {
      actionIdentifier: 'default',
      notification: { request: { identifier: 'cold', trigger: null, content: { data: {} } } },
    };
    let pending: typeof response | null = response;
    let cleared = 0,
      attempts = 0;
    const system = notificationSystemFixture({
      DEFAULT_ACTION_IDENTIFIER: 'default',
      addNotificationReceivedListener: () => ({ remove() {} }),
      addNotificationResponseReceivedListener: () => ({ remove() {} }),
      getLastNotificationResponseAsync: async () => pending,
      clearLastNotificationResponseAsync: async () => {
        cleared++;
        pending = null;
      },
    });
    const stop = system.listenNotificationEvents(
      () => {},
      () => {
        attempts++;
        if (refused === 'throws') throw Error('navigation not ready');
        return false;
      },
    );
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(attempts, 1);
    assert.equal(cleared, 0);
    assert.equal(pending, response);
    stop();
    const replacement = system.listenNotificationEvents(
      () => {},
      () => {
        attempts++;
        return true;
      },
    );
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(attempts, 2);
    assert.equal(cleared, 1);
    assert.equal(pending, null);
    replacement();
  });

test('a late initial response from a retired listener cannot consume the current tap', async () => {
  const response = {
    actionIdentifier: 'default',
    notification: { request: { identifier: 'current', trigger: null, content: { data: {} } } },
  };
  const old = deferred<typeof response | null>();
  let reads = 0,
    cleared = 0,
    oldTaps = 0,
    currentTaps = 0;
  const system = notificationSystemFixture({
    DEFAULT_ACTION_IDENTIFIER: 'default',
    addNotificationReceivedListener: () => ({ remove() {} }),
    addNotificationResponseReceivedListener: () => ({ remove() {} }),
    getLastNotificationResponseAsync: () =>
      ++reads === 1 ? old.promise : Promise.resolve(response),
    clearLastNotificationResponseAsync: async () => {
      cleared++;
    },
  });
  const previous = system.listenNotificationEvents(
    () => {},
    () => {
      oldTaps++;
      return true;
    },
  );
  previous();
  const current = system.listenNotificationEvents(
    () => {},
    () => {
      currentTaps++;
      return true;
    },
  );
  await new Promise<void>((resolve) => setImmediate(resolve));
  old.resolve(response);
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(oldTaps, 0);
  assert.equal(currentTaps, 1);
  assert.equal(cleared, 1);
  current();
});
