import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, draftSession, Driver, empty, entry, tick } from './store.fixtures.ts';
import { ClientStore } from './store.ts';

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
