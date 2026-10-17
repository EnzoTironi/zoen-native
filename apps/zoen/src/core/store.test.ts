import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import { completeProviderSession } from '../auth/complete-session.ts';
import type {
  Command,
  CommandResult,
  CoreDriver,
  CoreEvent,
  Snapshot,
  TimelineEntry,
  TimelinePage,
} from './types.ts';

const empty: Snapshot = {
  account: null,
  me: null,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  conversations: [],
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

class Driver implements CoreDriver {
  opening = async () => empty;
  operation: (command: Command) => Promise<unknown> = async () => empty;
  changed: ((event: CoreEvent) => void) | undefined;
  async open() {
    return this.opening();
  }
  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    return (await this.operation(command)) as CommandResult<C>;
  }
  async createAccount() {
    return empty;
  }
  subscribe(listener: (event: CoreEvent) => void) {
    this.changed = listener;
    return () => {
      if (this.changed === listener) this.changed = undefined;
    };
  }
  async close() {}
  async eraseDevice() {}
}

test('a lost provider bind response refreshes the retained identity so the same session can retry', async () => {
  const driver = new Driver();
  const account = {
    identity_id: 'a'.repeat(64),
    device_id: 'b'.repeat(64),
    name: 'Ana',
    handle: 'zoen_auto',
    relay_url: 'https://relay.example',
    registered: false,
    unlocked: true,
    setup_pending: true,
  };
  let durable = empty;
  let attempts = 0;
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return durable;
    assert.equal(command.op, 'provider_sign_in');
    attempts++;
    if (attempts === 1) {
      durable = { ...empty, account };
      throw new Error('Connection lost after server accepted the bind');
    }
    durable = { ...empty, account: { ...account, registered: true, setup_pending: false } };
    return durable.account;
  };
  const store = new ClientStore(driver);
  await store.open();
  const command = {
    op: 'provider_sign_in',
    relay_url: account.relay_url,
    proof_token: 'c'.repeat(64),
    name: account.name,
  } as const;
  await assert.rejects(() => store.request(command), /Connection lost/);
  assert.equal(store.getSnapshot().phase, 'onboarding');
  assert.equal(store.getSnapshot().snapshot?.account?.identity_id, account.identity_id);
  await completeProviderSession(
    {
      provider: 'google',
      proof_token: 'd'.repeat(64),
      expires_at_ms: Date.now() + 60_000,
      account: { kind: 'bound', identity_id: account.identity_id, handle: account.handle },
    },
    store.getSnapshot().snapshot?.account?.identity_id,
    (proof_token, name) => store.request({ ...command, proof_token, name }),
    () => {
      throw new Error('must retry the retained keys, not recover another identity');
    },
  );
  assert.equal(attempts, 2);
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().snapshot?.account?.device_id, account.device_id);
});

test('a late account open cannot replace the selected demo session', async () => {
  const driver = new Driver();
  const first = deferred<Snapshot>();
  const second = deferred<Snapshot>();
  let opens = 0;
  driver.opening = () => (++opens === 1 ? first.promise : second.promise);
  const store = new ClientStore(driver);
  const account = store.open('account');
  await Promise.resolve();
  await Promise.resolve();
  const demo = store.open('demo');
  await Promise.resolve();
  await Promise.resolve();
  second.resolve(empty);
  await demo;
  first.resolve({ ...empty, connection: { ...empty.connection, error: 'stale' } });
  await account;
  assert.equal(store.getSnapshot().mode, 'demo');
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().snapshot?.connection.error, null);
});

test('out-of-order timeline reads cannot overwrite the newer page', async () => {
  const driver = new Driver();
  const first = deferred<TimelinePage>();
  const second = deferred<TimelinePage>();
  let reads = 0;
  driver.operation = async (command) =>
    command.op === 'timeline' ? (++reads === 1 ? first.promise : second.promise) : empty;
  const store = new ClientStore(driver);
  await store.open('demo');
  const oldRead = store.loadTimeline('chat');
  const newRead = store.loadTimeline('chat');
  second.resolve({ entries: [], before: 'newer', has_more: false });
  await newRead;
  first.resolve({ entries: [], before: 'older', has_more: true });
  await oldRead;
  assert.equal(store.getSnapshot().timelines.chat?.before, 'newer');
});

test('a new session can refresh while an obsolete refresh is still pending', async () => {
  const driver = new Driver();
  const pending = deferred<Snapshot>();
  const store = new ClientStore(driver);
  await store.open('demo');
  driver.operation = async () => pending.promise;
  const oldRefresh = store.refresh();
  await store.open('demo');
  driver.operation = async () => ({ ...empty, connection: { ...empty.connection, pending: '7' } });
  await store.refresh();
  pending.resolve(empty);
  await oldRefresh;
  assert.equal(store.getSnapshot().snapshot?.connection.pending, '7');
});

test('refresh events during initial unlock do not expose an incomplete account', async () => {
  const driver = new Driver();
  const opening = deferred<Snapshot>();
  driver.opening = () => opening.promise;
  const store = new ClientStore(driver);
  const open = store.open('demo');
  await Promise.resolve();
  await Promise.resolve();
  driver.changed?.({ kind: 'connection' });
  assert.equal(store.getSnapshot().phase, 'opening');
  opening.resolve(empty);
  await open;
  assert.equal(store.getSnapshot().phase, 'ready');
});

const entry = (id: string) => ({ id }) as TimelineEntry;

test('sync refresh preserves the loaded reading window and refreshes older entries', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open('demo');
  const initial: TimelinePage = {
    entries: [entry('middle'), entry('last')],
    before: 'middle',
    has_more: true,
  };
  driver.operation = async () => initial;
  await store.loadTimeline('chat');
  driver.operation = async () => ({ entries: [entry('old')], before: 'old', has_more: true });
  await store.loadTimeline('chat', true);
  driver.operation = async (command) =>
    command.op !== 'timeline'
      ? empty
      : command.before === null
        ? { entries: [entry('last'), entry('new')], before: 'last', has_more: true }
        : {
            entries: [{ ...entry('old'), edited: true }, entry('middle')],
            before: 'old',
            has_more: false,
          };
  await store.loadTimeline('chat');
  const page = store.getSnapshot().timelines.chat!;
  assert.deepEqual(
    page.entries.map((message) => message.id),
    ['old', 'middle', 'last', 'new'],
  );
  assert.equal(page.entries[0]?.edited, true);
  assert.equal(page.has_more, false);
});

test('a quote jump reads until its target and coalesces concurrent older-page requests', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open('demo');
  driver.operation = async () => ({ entries: [entry('latest')], before: 'latest', has_more: true });
  await store.loadTimeline('chat');
  const older = deferred<TimelinePage>();
  let reads = 0;
  driver.operation = async () => {
    reads++;
    return older.promise;
  };
  const first = store.loadTimeline('chat', true);
  const second = store.loadTimeline('chat', true);
  assert.equal(reads, 1);
  older.resolve({ entries: [entry('target')], before: 'target', has_more: false });
  await Promise.all([first, second]);
  assert.equal(await store.revealMessage('chat', 'target'), 0);
  assert.equal(await store.revealMessage('chat', 'missing'), -1);
  assert.deepEqual(
    store.getSnapshot().timelines.chat?.entries.map((message) => message.id),
    ['target', 'latest'],
  );
});

const erasureAccount = {
  identity_id: 'a'.repeat(64),
  device_id: 'b'.repeat(64),
  name: 'Synthetic',
  handle: 'qa_erase',
  relay_url: 'https://relay.example',
  registered: true,
  unlocked: true,
  setup_pending: false,
};
const erasureScope = {
  identity_id: erasureAccount.identity_id,
  device_id: erasureAccount.device_id,
};

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
