import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, Driver, empty, erasureAccount, erasureScope } from './store.fixtures.ts';
import { ClientStore } from './store.ts';
import type { Snapshot } from './types.ts';

test('a stale erase confirmation cannot erase another account or device', async () => {
  const driver = new Driver();
  driver.opening = async () => ({ ...empty, account: erasureAccount });
  let calls = 0;
  driver.eraseDevice = async () => {
    calls++;
  };
  const store = new ClientStore(driver);
  await store.open();
  for (const expected of [
    { ...erasureScope, identity_id: 'c'.repeat(64) },
    { ...erasureScope, device_id: 'd'.repeat(64) },
  ])
    await assert.rejects(store.eraseDevice(expected), /conta mudou/);
  assert.equal(calls, 0);
  assert.equal(store.getSnapshot().phase, 'ready');
  await store.close();
});

test('erasure fences old results immediately, drains cleanup independently and reopens only the empty canonical account', async () => {
  const driver = new Driver();
  let durable = { ...empty, account: erasureAccount } as Snapshot;
  driver.opening = async () => durable;
  const old = deferred<unknown>();
  driver.operation = async (command) => (command.op === 'snapshot' ? durable : old.promise);
  const erase = deferred<void>();
  let calls = 0;
  driver.eraseDevice = async (expected = erasureScope) => {
    calls++;
    assert.deepEqual(expected, erasureScope);
    await erase.promise;
    durable = empty;
  };
  const store = new ClientStore(driver);
  await store.open();
  const oldResult = store.request({ op: 'new_mutation_id' });
  const oldRejected = assert.rejects(oldResult, /sessão mudou/);
  const expected = { ...erasureScope };
  const pending = store.eraseDevice(expected);
  expected.device_id = 'changed-after-confirmation';
  const same = store.eraseDevice(erasureScope);
  assert.equal(same, pending);
  assert.equal(store.getSnapshot().phase, 'erasing');
  assert.equal(store.getSnapshot().snapshot, null);
  assert.deepEqual(store.getSnapshot().timelines, {});
  await assert.rejects(store.request({ op: 'snapshot' }), /saída/);
  await assert.rejects(store.createAccount('Other', 'other', 'https://relay.example'), /saída/);
  old.resolve('obsolete');
  await oldRejected;
  erase.resolve();
  await pending;
  assert.equal(calls, 1);
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().erasePending, false);
  assert.equal(store.getSnapshot().snapshot?.account, null);
  await store.close();
});

test('failed projection cleanup retains its original owner through provider turnover and retries', async () => {
  const driver = new Driver();
  let durable: Snapshot = { ...empty, account: erasureAccount };
  driver.opening = async () => durable;
  let erasures = 0;
  driver.eraseDevice = async () => {
    erasures++;
    durable = empty;
  };
  const store = new ClientStore(driver);
  await store.open();
  let broken = true;
  let otherCleanup = 0;
  let originalCleanup = 0;
  const unregister = store.registerDeviceCleanup(async () => {
    originalCleanup++;
    if (broken) throw Error('widget unavailable');
  });
  store.registerDeviceCleanup(async () => {
    otherCleanup++;
  });
  let replacementCleanup = 0;
  const stop = store.subscribe(() => {
    if (store.getSnapshot().phase !== 'erasing') return;
    unregister();
    store.registerDeviceCleanup(async () => {
      replacementCleanup++;
    });
  });
  await assert.rejects(store.eraseDevice(erasureScope), /prévias/);
  assert.equal(erasures, 0);
  assert.equal(durable.account?.device_id, erasureScope.device_id);
  assert.equal(otherCleanup, 1);
  assert.equal(originalCleanup, 1);
  assert.equal(replacementCleanup, 0);
  assert.equal(store.getSnapshot().phase, 'failed');
  assert.equal(store.getSnapshot().erasePending, true);
  assert.equal(store.getSnapshot().snapshot, null);
  await assert.rejects(store.eraseDevice({ ...erasureScope, device_id: 'foreign' }), /conta mudou/);
  broken = false;
  await store.open();
  assert.equal(erasures, 1);
  assert.equal(otherCleanup, 2);
  assert.equal(originalCleanup, 2);
  assert.equal(replacementCleanup, 0);
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().erasePending, false);
  stop();
  await store.close();
});

test('failed canonical erasure does not reopen or restore the previous account; retry remains bound to the same device', async () => {
  const driver = new Driver();
  let durable: Snapshot = { ...empty, account: erasureAccount };
  let opens = 0;
  driver.opening = async () => {
    opens++;
    return durable;
  };
  let fail = true;
  driver.eraseDevice = async () => {
    if (fail) throw Error('synthetic vault failure');
    durable = empty;
  };
  const store = new ClientStore(driver);
  await store.open();
  await assert.rejects(store.eraseDevice(erasureScope), /vault failure/);
  assert.equal(opens, 1);
  assert.equal(store.getSnapshot().snapshot, null);
  assert.equal(store.getSnapshot().erasePending, true);
  fail = false;
  await store.eraseDevice(erasureScope);
  assert.equal(opens, 2);
  assert.equal(store.getSnapshot().phase, 'onboarding');
  await store.close();
});
