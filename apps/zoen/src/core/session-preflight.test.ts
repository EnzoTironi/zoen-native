import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import type { Command, CommandResult, CoreDriver, CoreEvent, Snapshot } from './types.ts';

const blank: Snapshot = {
  account: null,
  me: null,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  conversations: [],
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
};
const account: Snapshot = {
  ...blank,
  account: {
    identity_id: 'a'.repeat(64),
    device_id: 'b'.repeat(64),
    name: 'Synthetic',
    handle: 'synthetic',
    relay_url: 'https://relay.example',
    registered: true,
    unlocked: true,
    setup_pending: false,
  },
};

class Driver implements CoreDriver {
  snapshot = account;
  readonly calls: string[] = [];
  listener: ((event: CoreEvent) => void) | null = null;
  async open() {
    this.calls.push('open');
    return this.snapshot;
  }
  async close() {
    this.calls.push('close');
  }
  async createAccount() {
    this.calls.push('create');
    return this.snapshot;
  }
  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    this.calls.push(command.op);
    const value =
      command.op === 'timeline' ? { entries: [], before: null, has_more: false } : this.snapshot;
    return value as CommandResult<C>;
  }
  subscribe(listener: (event: CoreEvent) => void) {
    this.calls.push('subscribe');
    this.listener = listener;
    return () => {
      this.calls.push('unsubscribe');
      if (this.listener === listener) this.listener = null;
    };
  }
  async eraseDevice() {
    this.calls.push('erase');
    this.snapshot = blank;
  }
}

test('a rejecting replacement preflight preserves the entire session before any cleanup or publication', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  await store.loadTimeline('synthetic-chat');
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const listener = driver.listener;
  const rejection = new Error('Close the drawing first.');
  let checks = 0;
  let retirements = 0;
  let publications = 0;
  const stopFirst = store.registerSessionPreflight(() => {
    checks++;
  });
  const stop = store.registerSessionPreflight(() => {
    checks++;
    throw rejection;
  });
  store.registerSessionCleanup(async () => {
    retirements++;
  });
  const unsubscribe = store.subscribe(() => publications++);
  driver.calls.length = 0;
  const pending = store.open('demo');
  assert.equal(checks, 2, 'both checks execute before open yields');
  assert.equal(store.getSnapshot(), before);
  await assert.rejects(pending, (error) => error === rejection);
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(driver.listener, listener);
  assert.deepEqual(driver.calls, []);
  assert.equal(retirements, 0);
  assert.equal(publications, 0);
  unsubscribe();
  stopFirst();
  stop();
  await store.close();
});

test('a rejecting account creation preflight cannot create keys or replace the current snapshot', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const rejection = new Error('Keep the current drawing open.');
  const stop = store.registerSessionPreflight(() => {
    throw rejection;
  });
  driver.calls.length = 0;
  await assert.rejects(
    store.createAccount('New', 'new_owner', 'https://relay.example'),
    (error) => error === rejection,
  );
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.deepEqual(driver.calls, []);
  stop();
  await store.createAccount('New', 'new_owner', 'https://relay.example');
  assert.deepEqual(driver.calls, ['create']);
  await store.close();
});

test('unregistering a preflight allows the normal retirement and replacement sequence', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const epoch = store.sessionEpoch;
  const stop = store.registerSessionPreflight(() => {
    throw new Error('Drawing open');
  });
  store.registerSessionCleanup(async (discard) => {
    assert.equal(discard, false);
    driver.calls.push('retire');
  });
  stop();
  stop();
  driver.calls.length = 0;
  await store.open('demo');
  assert.deepEqual(driver.calls, ['unsubscribe', 'retire', 'close', 'subscribe', 'open']);
  assert.ok(store.sessionEpoch > epoch);
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().mode, 'demo');
  await store.close();
});

test('ordinary close honors preflight before cleanup, admission fences or publication', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const listener = driver.listener;
  let retired = false;
  let publications = 0;
  store.registerSessionCleanup(async () => {
    retired = true;
  });
  const stop = store.registerSessionPreflight(() => {
    throw new Error('Keep the current editor mounted.');
  });
  const unsubscribe = store.subscribe(() => publications++);
  driver.calls.length = 0;
  await assert.rejects(store.close(), /current editor mounted/);
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(store.isSessionRetiring, false);
  assert.equal(driver.listener, listener);
  assert.equal(retired, false);
  assert.equal(publications, 0);
  assert.deepEqual(driver.calls, []);
  unsubscribe();
  stop();
  await store.close();
});

test('explicit provider teardown bypasses replacement preflights and still fences and closes the owner', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const epoch = store.sessionEpoch;
  let checks = 0;
  store.registerSessionPreflight(() => {
    checks++;
    throw new Error('Drawing open');
  });
  store.registerSessionCleanup(async (discard) => {
    assert.equal(discard, false);
    driver.calls.push('retire');
  });
  driver.calls.length = 0;
  await store.close({ force: true });
  assert.equal(checks, 0);
  assert.deepEqual(driver.calls, ['unsubscribe', 'retire', 'close']);
  assert.ok(store.sessionEpoch > epoch);
  assert.equal(store.getSnapshot().snapshot, null);
  assert.equal(driver.listener, null);
});

test('confirmed device erasure bypasses preflights through the final empty-account reopen', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  let checks = 0;
  const stopPreflight = store.registerSessionPreflight(() => {
    checks++;
    throw new Error('Drawing open');
  });
  const stopCleanup = store.registerSessionCleanup(async (discard) => {
    driver.calls.push(discard ? 'discard' : 'retire');
  });
  driver.calls.length = 0;
  await store.eraseDevice({
    identity_id: account.account!.identity_id,
    device_id: account.account!.device_id,
  });
  assert.equal(checks, 0);
  assert.deepEqual(driver.calls, [
    'unsubscribe',
    'discard',
    'erase',
    'retire',
    'close',
    'subscribe',
    'open',
  ]);
  assert.equal(store.getSnapshot().snapshot?.account, null);
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().erasePending, false);
  stopCleanup();
  stopPreflight();
  await store.close();
});
