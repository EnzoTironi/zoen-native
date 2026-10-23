import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, draftSession, Driver, empty, erasureScope, tick } from './store.fixtures.ts';
import { ClientStore } from './store.ts';
import type { Snapshot } from './types.ts';

test('cleanup and publication reentrancy coalesce into the already confirmed erasure', async () => {
  const driver = new Driver();
  let durable: Snapshot = draftSession();
  driver.opening = async () => durable;
  const store = new ClientStore(driver);
  await store.open();
  const observed: Promise<void>[] = [];
  let reopened: Promise<void> | undefined;
  let erasures = 0;
  const stopCleanup = store.registerSessionCleanup(async (discard) => {
    if (discard) observed.push(store.eraseDevice(erasureScope));
  });
  const unsubscribe = store.subscribe(() => {
    if (reopened || store.getSnapshot().phase !== 'erasing') return;
    reopened = Promise.resolve();
    observed.push(store.eraseDevice(erasureScope));
    reopened = store.open('demo');
  });
  driver.eraseDevice = async () => {
    erasures++;
    durable = empty;
  };
  const erasing = store.eraseDevice(erasureScope);
  assert.equal(observed.length, 2);
  for (const pending of observed) assert.equal(pending, erasing);
  await Promise.all([erasing, reopened]);
  assert.equal(erasures, 1);
  assert.equal(store.getSnapshot().mode, 'account');
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().snapshot, empty);
  unsubscribe();
  stopCleanup();
  await store.close();
});

test('confirmed erase supersedes a graceful cleanup that needs erasure to finish', async () => {
  const driver = new Driver();
  let durable: Snapshot = draftSession();
  driver.opening = async () => durable;
  const store = new ClientStore(driver);
  await store.open();
  const preserve = deferred<void>();
  const events: string[] = [];
  const stop = store.registerSessionCleanup((discard) => {
    events.push(discard ? 'discard' : 'preserve');
    return discard ? Promise.resolve() : preserve.promise;
  });
  driver.close = async () => {
    events.push('close');
  };
  driver.eraseDevice = async () => {
    events.push('erase');
    durable = empty;
    preserve.resolve();
  };
  const replacement = store.open('demo');
  const erasing = store.eraseDevice(erasureScope);
  assert.equal(store.getSnapshot().phase, 'erasing');
  await Promise.all([replacement, erasing]);
  assert.deepEqual(events, ['preserve', 'discard', 'erase', 'preserve', 'close']);
  assert.equal(store.getSnapshot().mode, 'account');
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().snapshot, empty);
  stop();
  await store.close();
});

test('confirmed erase releases an in-flight native close and drains it before canonical reopen', async () => {
  const driver = new Driver();
  let durable: Snapshot = draftSession();
  driver.opening = async () => durable;
  const store = new ClientStore(driver);
  await store.open();
  const seal = deferred<void>();
  const entered = deferred<void>();
  const events: string[] = [];
  let closes = 0;
  driver.close = async () => {
    if (++closes === 1) {
      events.push('old-close');
      entered.resolve();
      await seal.promise;
      events.push('old-close-settled');
    } else events.push('empty-close');
  };
  driver.eraseDevice = async () => {
    events.push('erase');
    durable = empty;
    seal.resolve();
  };
  driver.opening = async () => {
    events.push('empty-open');
    return durable;
  };
  const closing = store.close();
  await entered.promise;
  const erasing = store.eraseDevice(erasureScope);
  await Promise.all([closing, erasing]);
  assert.deepEqual(events, [
    'old-close',
    'erase',
    'old-close-settled',
    'empty-close',
    'empty-open',
  ]);
  assert.equal(store.getSnapshot().mode, 'account');
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().notice, null);
  await store.close();
});

test('erasure retry retains an obsolete native close drain until it settles before reopening', async () => {
  const driver = new Driver();
  let durable: Snapshot = draftSession();
  driver.opening = async () => durable;
  const store = new ClientStore(driver);
  await store.open();
  const seal = deferred<void>();
  const entered = deferred<void>();
  const events: string[] = [];
  let closes = 0;
  let erasures = 0;
  driver.close = async () => {
    if (++closes === 1) {
      events.push('old-close');
      entered.resolve();
      await seal.promise;
    } else events.push('empty-close');
  };
  driver.eraseDevice = async () => {
    events.push('erase');
    if (++erasures === 1) throw new Error('temporary erasure failure');
    durable = empty;
  };
  driver.opening = async () => {
    events.push('empty-open');
    return durable;
  };
  const closing = store.close();
  await entered.promise;
  await assert.rejects(store.eraseDevice(erasureScope), /temporary erasure failure/);
  assert.equal(store.getSnapshot().erasePending, true);
  const retry = store.eraseDevice(erasureScope);
  await tick();
  assert.deepEqual(events, ['old-close', 'erase', 'erase']);
  assert.equal(store.getSnapshot().phase, 'erasing');
  seal.reject(new Error('obsolete native close rejected'));
  await Promise.all([closing, retry]);
  assert.deepEqual(events, ['old-close', 'erase', 'erase', 'empty-close', 'empty-open']);
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().mode, 'account');
  assert.equal(store.getSnapshot().snapshot, empty);
  assert.equal(store.getSnapshot().notice, null);
  await store.close();
});
