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
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, failed) => {
    resolve = done;
    reject = failed;
  });
  return { promise, resolve, reject };
}
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

class Driver implements CoreDriver {
  durableMessageDrafts?: boolean;
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
  async close(_options?: { force?: boolean }) {}
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
  await tick();
  const demo = store.open('demo');
  await tick();
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
  await tick();
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

function unreadSnapshot(unread: number): Snapshot {
  return {
    ...empty,
    account: erasureAccount,
    conversations: [
      {
        id: 'read-chat',
        title: 'Read QA',
        kind: 'Group',
        privacy: 'EndToEnd',
        members: [],
        counterpart: null,
        last_preview: 'A received file',
        last_author: null,
        last_at_ms: '1',
        unread,
        last_delivery: null,
        pinned: false,
        archived: false,
        muted: false,
        muted_until_ms: null,
        pending_requests: 0,
        event_count: '2',
      },
    ],
  };
}

test('the mutation path clears a durable read badge without relying on a core event or changing the last entry', async () => {
  const driver = new Driver();
  let durable = unreadSnapshot(1);
  const page: TimelinePage = {
    entries: [entry('received-file'), entry('file-version')],
    before: null,
    has_more: false,
  };
  let marks = 0;
  let snapshots = 0;
  let timelines = 0;
  driver.opening = async () => durable;
  driver.operation = async (command) => {
    if (command.op === 'mark_read') {
      marks++;
      durable = unreadSnapshot(0);
      return null;
    }
    if (command.op === 'snapshot') {
      snapshots++;
      return durable;
    }
    assert.equal(command.op, 'timeline');
    timelines++;
    return page;
  };
  const store = new ClientStore(driver);
  await store.open();
  store.selectSpace('read-chat');
  await store.loadTimeline('read-chat');
  const previousReads = timelines;
  const last = store.getSnapshot().timelines['read-chat']?.entries.at(-1)?.id;

  // A direct bridge call really does persist the read, but no network event is
  // guaranteed to refresh this device's existing snapshot afterward.
  await store.driver.request({ op: 'mark_read', space_id: 'read-chat' });
  assert.equal(durable.conversations[0]?.unread, 0);
  assert.equal(store.getSnapshot().snapshot?.conversations[0]?.unread, 1);
  await store.request({ op: 'mark_read', space_id: 'read-chat' });
  assert.equal(store.getSnapshot().snapshot?.conversations[0]?.unread, 0);
  assert.equal(store.getSnapshot().timelines['read-chat']?.entries.at(-1)?.id, last);
  assert.equal(marks, 2, 'one raw read and one mutation-path read, without another automatic read');
  assert.equal(snapshots, 1);
  assert.equal(timelines, previousReads + 1, 'the active reading window is refreshed once');
  await store.close();
});

test('a failed read mutation preserves the badge and publishes a current-session notice', async () => {
  const driver = new Driver();
  driver.opening = async () => unreadSnapshot(1);
  driver.operation = async (command) => {
    assert.equal(command.op, 'mark_read');
    throw new Error('synthetic read storage failure');
  };
  const store = new ClientStore(driver);
  await store.open();
  await assert.rejects(
    store.request({ op: 'mark_read', space_id: 'read-chat' }),
    /read storage failure/,
  );
  assert.equal(store.getSnapshot().snapshot?.conversations[0]?.unread, 1);
  assert.equal(store.getSnapshot().notice, 'synthetic read storage failure');
  await store.close();
});

test('an old read completion cannot refresh or publish an error into a replacement session', async () => {
  const driver = new Driver();
  driver.opening = async () => unreadSnapshot(1);
  const store = new ClientStore(driver);
  await store.open();
  const mark = deferred<unknown>();
  let snapshots = 0;
  driver.operation = async (command) => {
    if (command.op === 'mark_read') return mark.promise;
    assert.equal(command.op, 'snapshot');
    snapshots++;
    return unreadSnapshot(0);
  };
  const oldRead = store.request({ op: 'mark_read', space_id: 'read-chat' });
  driver.opening = async () => empty;
  await store.open('demo');
  mark.resolve(null);
  await assert.rejects(oldRead, /sessão mudou/);
  assert.equal(snapshots, 0);
  assert.equal(store.getSnapshot().mode, 'demo');
  assert.equal(store.getSnapshot().snapshot, empty);
  assert.equal(store.getSnapshot().notice, null);
  await store.close();
});

test('session retirement stops admission before publication and drains before closing the native realm', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open('demo');
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const saved = deferred<void>();
  const calls: string[] = [];
  driver.close = async () => {
    calls.push('native-close');
  };
  store.registerSessionCleanup((discard) => {
    assert.equal(discard, false);
    calls.push('retired');
    return saved.promise;
  });
  const closing = store.close();
  assert.deepEqual(calls, ['retired']);
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(store.isSessionRetiring, true);
  await assert.rejects(store.request({ op: 'snapshot' }), /sessão está sendo encerrada/);
  saved.resolve();
  await closing;
  assert.deepEqual(calls, ['retired', 'native-close']);
  assert.equal(store.getSnapshot().phase, 'opening');
  assert.equal(store.getSnapshot().snapshot, null);
  assert.ok(store.sessionEpoch > epoch);
});

test('a failed draft drain keeps the old realm open and can be retried before replacement', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open('demo');
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  let fail = true;
  let closes = 0;
  let opens = 0;
  driver.close = async () => {
    closes++;
  };
  driver.opening = async () => {
    opens++;
    return empty;
  };
  const stop = store.registerSessionCleanup(async () => {
    if (fail) throw new Error('synthetic draft save failure');
  });
  await store.open('demo');
  assert.equal(closes, 0);
  assert.equal(opens, 0);
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().snapshot, before.snapshot);
  assert.equal(store.getSnapshot().mode, before.mode);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(store.getSnapshot().notice, 'synthetic draft save failure');
  fail = false;
  await store.open('demo');
  assert.equal(closes, 1);
  assert.equal(opens, 1);
  assert.equal(store.getSnapshot().phase, 'ready');
  stop();
  await store.close();
});

test('replacement keeps the exact visible owner through cleanup and native close while fencing new work', async () => {
  const driver = new Driver();
  const original = draftSession();
  driver.opening = async () => original;
  driver.operation = async (command) =>
    command.op === 'timeline'
      ? { entries: [entry('mounted')], before: null, has_more: false }
      : original;
  const store = new ClientStore(driver);
  await store.open();
  await store.loadTimeline('read-chat');
  const oldChannel = store.messageDrafts.channel('read-chat', null)!;
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const cleanup = deferred<void>();
  const close = deferred<void>();
  const closing = deferred<void>();
  const late = deferred<unknown>();
  let admittedCleanup = false;
  let newRequests = 0;
  let opens = 0;
  driver.operation = async (command) => {
    if (command.op === 'new_mutation_id') return late.promise;
    newRequests++;
    assert.equal(command.op, 'snapshot');
    return original;
  };
  const obsolete = assert.rejects(store.request({ op: 'new_mutation_id' }), /sessão mudou/);
  const stopCleanup = store.registerSessionCleanup(async () => {
    await cleanup.promise;
    assert.equal(store.getSnapshot().snapshot, original);
    assert.equal(store.sessionEpoch, epoch, 'the final seal still has its exact owner');
    await store.driver.request({ op: 'snapshot' });
    admittedCleanup = true;
  });
  driver.close = async () => {
    closing.resolve();
    await close.promise;
  };
  driver.opening = async () => {
    opens++;
    return empty;
  };
  const replacement = store.open('demo');
  assert.equal(store.isSessionRetiring, true);
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(store.messageDrafts.channel('read-chat', null), null);
  assert.equal(oldChannel.controller.setText(oldChannel.scope, 'too late'), false);
  await assert.rejects(store.request({ op: 'snapshot' }), /sessão está sendo encerrada/);
  await assert.rejects(store.loadTimeline('read-chat'), /sessão está sendo encerrada/);
  await assert.rejects(store.createAccount('New', 'new', 'https://relay.example'), /encerrada/);
  assert.throws(() => store.assertExternalNavigationAllowed(), /encerrada/);
  await store.refresh();
  late.resolve('obsolete');
  await obsolete;
  assert.equal(newRequests, 0);
  assert.equal(store.getSnapshot(), before, 'obsolete results cannot publish during the seal');
  cleanup.resolve();
  await closing.promise;
  assert.equal(admittedCleanup, true);
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(store.messageDrafts.channel('read-chat', null), null);
  assert.equal(opens, 0);
  close.resolve();
  await replacement;
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().mode, 'demo');
  assert.equal(store.getSnapshot().snapshot, empty);
  assert.equal(store.isSessionRetiring, false);
  assert.ok(store.sessionEpoch > epoch);
  assert.equal(opens, 1);
  stopCleanup();
  await store.close();
});

test('cleanup failure drains every admitted owner and retains the mounted session for a close retry', async () => {
  const driver = new Driver();
  driver.opening = async () => draftSession();
  const store = new ClientStore(driver);
  await store.open();
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const draining = deferred<void>();
  let fail = true;
  let closes = 0;
  driver.close = async () => {
    closes++;
  };
  const stopFailure = store.registerSessionCleanup(async () => {
    if (fail) throw new Error('final source could not be saved');
  });
  const stopDrain = store.registerSessionCleanup(() => draining.promise);
  const rejected = assert.rejects(store.close(), /final source could not be saved/);
  await tick();
  assert.equal(store.getSnapshot(), before, 'a failed owner cannot abandon another drain');
  assert.equal(closes, 0);
  draining.resolve();
  await rejected;
  assert.equal(store.getSnapshot().phase, 'ready');
  assert.equal(store.getSnapshot().mode, before.mode);
  assert.equal(store.getSnapshot().snapshot, before.snapshot);
  assert.equal(store.getSnapshot().timelines, before.timelines);
  assert.equal(store.sessionEpoch, epoch);
  assert.equal(store.getSnapshot().notice, 'final source could not be saved');
  assert.equal(closes, 0);
  fail = false;
  await store.close();
  assert.equal(closes, 1);
  assert.equal(store.getSnapshot().snapshot, null);
  stopFailure();
  stopDrain();
});

test('native close failure cannot unmount or replace the old session and both actions can retry', async () => {
  for (const action of ['replace', 'close'] as const) {
    const driver = new Driver();
    driver.opening = async () => draftSession();
    const store = new ClientStore(driver);
    await store.open();
    const before = store.getSnapshot();
    const epoch = store.sessionEpoch;
    const seal = deferred<void>();
    const entered = deferred<void>();
    let opens = 0;
    driver.opening = async () => {
      opens++;
      return empty;
    };
    driver.close = async () => {
      entered.resolve();
      await seal.promise;
    };
    const pending = action === 'replace' ? store.open('demo') : store.close();
    const checked = action === 'close' ? assert.rejects(pending, /final seal failed/) : pending;
    await entered.promise;
    assert.equal(store.getSnapshot(), before);
    assert.equal(store.sessionEpoch, epoch);
    assert.equal(opens, 0);
    seal.reject(new Error('final seal failed'));
    await checked;
    assert.equal(store.getSnapshot().phase, 'ready');
    assert.equal(store.getSnapshot().snapshot, before.snapshot);
    assert.equal(store.getSnapshot().timelines, before.timelines);
    assert.equal(store.getSnapshot().mode, before.mode);
    assert.equal(store.getSnapshot().notice, 'final seal failed');
    assert.equal(store.sessionEpoch, epoch);
    assert.equal(opens, 0);
    driver.close = async () => {};
    if (action === 'replace') {
      await store.open('demo');
      assert.equal(store.getSnapshot().mode, 'demo');
      assert.equal(opens, 1);
    } else await store.close();
    assert.ok(store.sessionEpoch > epoch);
    await store.close();
  }
});

test('concurrent replacements share one retirement and only the latest selected session opens', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const cleanup = deferred<void>();
  let retirements = 0;
  let closes = 0;
  let opens = 0;
  const stop = store.registerSessionCleanup(() => {
    retirements++;
    return cleanup.promise;
  });
  driver.close = async () => {
    closes++;
  };
  driver.opening = async () => {
    opens++;
    return empty;
  };
  const first = store.open('account');
  const latest = store.open('demo');
  assert.equal(retirements, 1);
  cleanup.resolve();
  await Promise.all([first, latest]);
  assert.equal(closes, 1);
  assert.equal(opens, 1);
  assert.equal(store.getSnapshot().mode, 'demo');
  assert.equal(store.getSnapshot().phase, 'ready');
  stop();
  await store.close();
});

test('a newer close supersedes a pending replacement without reopening the retired owner', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open('demo');
  const cleanup = deferred<void>();
  const stop = store.registerSessionCleanup(() => cleanup.promise);
  let opens = 0;
  let closes = 0;
  driver.opening = async () => {
    opens++;
    return empty;
  };
  driver.close = async () => {
    closes++;
  };
  const replacing = store.open();
  const closing = store.close();
  cleanup.resolve();
  await Promise.all([replacing, closing]);
  assert.equal(closes, 1);
  assert.equal(opens, 0);
  assert.equal(store.getSnapshot().mode, 'demo');
  assert.equal(store.getSnapshot().snapshot, null);
  stop();
});

test('a newer open supersedes a pending close while sharing the same final seal', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const seal = deferred<void>();
  const entered = deferred<void>();
  let closes = 0;
  let opens = 0;
  driver.close = async () => {
    closes++;
    entered.resolve();
    await seal.promise;
  };
  driver.opening = async () => {
    opens++;
    return empty;
  };
  const closing = store.close();
  await entered.promise;
  const opening = store.open('demo');
  assert.equal(closes, 1);
  seal.resolve();
  await Promise.all([closing, opening]);
  assert.equal(closes, 1);
  assert.equal(opens, 1);
  assert.equal(store.getSnapshot().mode, 'demo');
  assert.equal(store.getSnapshot().phase, 'ready');
  await store.close();
});

test('explicit teardown forwards force after admitted cleanup while ordinary close stays graceful', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const cleanup = deferred<void>();
  const options: ({ force?: boolean } | undefined)[] = [];
  const stop = store.registerSessionCleanup(() => cleanup.promise);
  driver.close = async (value) => {
    options.push(value);
  };
  const forced = store.close({ force: true });
  assert.deepEqual(options, [], 'already admitted JS saves still drain before native teardown');
  cleanup.resolve();
  await forced;
  assert.deepEqual(options, [{ force: true }]);
  stop();
  await store.open();
  assert.deepEqual(options, [{ force: true }, undefined]);
  await store.close();
  assert.deepEqual(options, [{ force: true }, undefined, undefined]);
});

test('forced teardown immediately escalates an in-flight graceful native seal', async () => {
  const driver = new Driver();
  driver.opening = async () => draftSession();
  const store = new ClientStore(driver);
  await store.open();
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const seal = deferred<void>();
  const entered = deferred<void>();
  const released = deferred<void>();
  const options: ({ force?: boolean } | undefined)[] = [];
  driver.close = async (value) => {
    options.push(value);
    if (value?.force) {
      seal.reject(new Error('graceful seal superseded by forced native teardown'));
      await released.promise;
    } else {
      entered.resolve();
      await seal.promise;
    }
  };
  const graceful = store.close();
  await entered.promise;
  const forced = store.close({ force: true });
  assert.deepEqual(
    options,
    [undefined, { force: true }],
    'force reaches native before seal returns',
  );
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  released.resolve();
  await Promise.all([graceful, forced]);
  assert.equal(store.getSnapshot().snapshot, null);
  assert.equal(store.getSnapshot().notice, null);
  assert.equal(store.sessionEpoch, epoch + 1, 'coalesced native close retires one exact owner');
});

test('a synchronous subscriber selecting a session supersedes an older retirement publication', async () => {
  for (const phase of ['ready', 'opening'] as const) {
    const driver = new Driver();
    driver.opening = async () => draftSession();
    const store = new ClientStore(driver);
    await store.open();
    let selected: Promise<void> | undefined;
    let opens = 0;
    driver.opening = async () => {
      opens++;
      return empty;
    };
    const unsubscribe = store.subscribe(() => {
      if (selected || store.getSnapshot().phase !== phase) return;
      // Install the guard before calling into Store, since publication is synchronous.
      selected = Promise.resolve();
      selected = store.open('demo');
    });
    await store.open('account');
    assert.ok(selected);
    await selected;
    assert.equal(opens, 1);
    assert.equal(store.getSnapshot().mode, 'demo');
    assert.equal(store.getSnapshot().phase, 'ready');
    unsubscribe();
    await store.close();
  }
});

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

function draftSession(): Snapshot {
  const snapshot = unreadSnapshot(0);
  return {
    ...snapshot,
    conversations: snapshot.conversations.map((conversation) => ({
      ...conversation,
      members: [
        {
          id: erasureAccount.identity_id,
          kind: 'Person',
          name: 'QA',
          handle: 'qa',
          initials: 'QA',
          tint_hex: '#194D2A',
          glyph: null,
          bio: '',
          owner_id: null,
          owner_name: null,
          owner_tint_hex: null,
          photo_sha256: null,
          photo_ready: false,
          photo_mime: null,
          is_me: true,
          is_mine: false,
        },
      ],
    })),
  };
}

test('native draft writes avoid snapshot polling and survive controller retirement with exact scoped commands', async () => {
  const driver = new Driver();
  driver.durableMessageDrafts = true;
  driver.opening = async () => draftSession();
  let durable = { revision: '0', text: '', reply_to: null as string | null };
  const commands: Command[] = [];
  driver.operation = async (command) => {
    commands.push(command);
    assert.equal(
      'expected_identity_id' in command && command.expected_identity_id,
      erasureAccount.identity_id,
    );
    assert.equal(
      'expected_device_id' in command && command.expected_device_id,
      erasureAccount.device_id,
    );
    assert.equal('thread_root_id' in command && command.thread_root_id, null);
    if (command.op === 'message_draft') return durable;
    assert.equal(command.op, 'save_message_draft');
    if (command.op !== 'save_message_draft') throw Error('unexpected command');
    assert.equal(command.expected_revision, durable.revision);
    durable = { revision: 'opaque-next', text: command.text, reply_to: command.reply_to };
    return durable;
  };
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.messageDrafts.channel('read-chat', null)!;
  await channel.controller.activate(channel.scope);
  assert.equal(channel.controller.setText(channel.scope, '  Café 🌤️\r\nAinda não enviado  '), true);
  await store.close();
  assert.equal(commands.filter((command) => command.op === 'save_message_draft').length, 1);
  assert.equal(
    commands.some((command) => command.op === 'snapshot'),
    false,
  );
  assert.equal(store.messageDrafts.isCurrent(channel), false);
  assert.equal(channel.controller.setText(channel.scope, 'stale callback'), false);
  await store.open();
  const restored = store.messageDrafts.channel('read-chat', null)!;
  assert.notEqual(restored.controller, channel.controller);
  await restored.controller.activate(restored.scope);
  assert.equal(
    restored.controller.getSnapshot(restored.scope).text,
    '  Café 🌤️\r\nAinda não enviado  ',
  );
  assert.notEqual(restored.scope.owner.sessionEpoch, channel.scope.owner.sessionEpoch);
  await store.close();
});

test('device erasure cancels queued draft text and drains an admitted native save before wiping', async () => {
  const driver = new Driver();
  driver.durableMessageDrafts = true;
  let erased = false;
  driver.opening = async () => (erased ? empty : draftSession());
  const save = deferred<unknown>();
  const admitted = deferred<void>();
  const writes: string[] = [];
  driver.operation = async (command) => {
    if (command.op === 'message_draft') return { revision: '0', text: '', reply_to: null };
    assert.equal(command.op, 'save_message_draft');
    if (command.op !== 'save_message_draft') throw Error('unexpected command');
    assert.equal(erased, false);
    writes.push(command.text);
    admitted.resolve();
    return save.promise;
  };
  driver.eraseDevice = async () => {
    erased = true;
    writes.push('wipe');
  };
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.messageDrafts.channel('read-chat', null)!;
  await channel.controller.activate(channel.scope);
  channel.controller.setText(channel.scope, 'admitted');
  await admitted.promise;
  channel.controller.setText(channel.scope, 'queued after admitted');
  const erasing = store.eraseDevice(erasureScope);
  assert.equal(store.messageDrafts.isCurrent(channel), false);
  assert.equal(channel.controller.setText(channel.scope, 'late'), false);
  assert.equal(erased, false);
  save.resolve({ revision: 'saved-before-wipe', text: 'admitted', reply_to: null });
  await erasing;
  assert.deepEqual(writes, ['admitted', 'wipe']);
  assert.equal(channel.controller.getSnapshot(channel.scope).text, '');
  assert.equal(store.getSnapshot().phase, 'onboarding');
  await store.close();
});
