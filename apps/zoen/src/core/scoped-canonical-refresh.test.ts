import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import type { Command, CommandResult, CoreDriver, CoreEvent, Snapshot } from './types.ts';

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
const snapshot: Snapshot = {
  account: null,
  me: null,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  conversations: [],
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
};
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
class Driver implements CoreDriver {
  changed?: (event: CoreEvent) => void;
  reads: string[] = [];
  snapshotRead: () => Promise<void> = async () => {};
  open = async () => snapshot;
  createAccount = async () => snapshot;
  close = async () => {};
  eraseDevice = async () => {};
  subscribe(listener: (event: CoreEvent) => void) {
    this.changed = listener;
    return () => {
      if (this.changed === listener) this.changed = undefined;
    };
  }
  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    if (command.op === 'snapshot') {
      this.reads.push('snapshot');
      await this.snapshotRead();
      return { ...snapshot } as CommandResult<C>;
    }
    if (command.op === 'timeline') {
      this.reads.push(command.space_id);
      return { entries: [], before: null, has_more: false } as CommandResult<C>;
    }
    assert.equal(command.op, 'delete_message');
    return undefined as CommandResult<C>;
  }
}
async function fixture() {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open('demo');
  store.selectSpace('chat-A');
  await tick();
  driver.reads = [];
  const changes: Array<readonly string[] | undefined> = [];
  const unsubscribe = store.subscribeCanonicalChanges((spaces) => changes.push(spaces));
  return { driver, store, changes, unsubscribe };
}

test('another chat refreshes the directory without rereading or replacing the open reading window', async () => {
  const f = await fixture();
  const before = f.store.getSnapshot().timelines;
  const gate = deferred();
  f.driver.snapshotRead = () => gate.promise;
  const spaces = ['chat-B'];
  f.driver.changed?.({ kind: 'change', spaceIds: spaces });
  spaces.push('chat-A');
  assert.deepEqual(f.changes, [['chat-B']], 'scope is copied and published before the read waits');
  assert.ok(Object.isFrozen(f.changes[0]));
  gate.resolve();
  await tick();
  assert.deepEqual(f.driver.reads, ['snapshot']);
  assert.equal(f.store.getSnapshot().timelines, before);
  await f.store.close();
});

test('relevant invalidation is immediate even behind another chat read, and queued scopes are merged', async () => {
  const f = await fixture();
  const gate = deferred();
  let reads = 0;
  f.driver.snapshotRead = async () => {
    if (++reads === 1) await gate.promise;
  };
  const first = f.store.refresh(['chat-B']);
  const second = f.store.refresh(['chat-A']);
  f.store.refresh(['chat-C']);
  assert.equal(second, first);
  assert.deepEqual(f.changes, [['chat-B'], ['chat-A'], ['chat-C']]);
  assert.deepEqual(f.driver.reads, ['snapshot']);
  gate.resolve();
  await first;
  assert.deepEqual(f.driver.reads, ['snapshot', 'snapshot', 'chat-A']);
  assert.equal(
    f.changes.length,
    3,
    'the already-announced timeline read does not invalidate again',
  );
  await f.store.close();
});

test('unknown changes dominate scoped coalesced changes in either order', async () => {
  for (const pending of [
    [undefined, ['chat-C']],
    [['chat-C'], []],
  ] as const) {
    const f = await fixture();
    const gate = deferred();
    let reads = 0;
    f.driver.snapshotRead = async () => {
      if (++reads === 1) await gate.promise;
    };
    const first = f.store.refresh(['chat-B']);
    for (const scope of pending) f.store.refresh(scope);
    gate.resolve();
    await first;
    assert.deepEqual(f.driver.reads, ['snapshot', 'snapshot', 'chat-A']);
    assert.ok(f.changes.includes(undefined), 'unknown and empty scopes remain broad invalidations');
    await f.store.close();
  }
});

test('offline command refresh and explicit history reads preserve their own chat scope', async () => {
  const f = await fixture();
  const before = f.store.getSnapshot().timelines['chat-A'];
  await f.store.request({ op: 'delete_message', space_id: 'chat-B', message_id: 'old-message' });
  assert.deepEqual(f.changes, [['chat-B']]);
  assert.deepEqual(f.driver.reads, ['snapshot']);
  await f.store.loadTimeline('chat-B');
  assert.deepEqual(f.changes, [['chat-B'], ['chat-B']]);
  assert.equal(f.store.getSnapshot().timelines['chat-A'], before);
  assert.deepEqual(f.driver.reads, ['snapshot', 'chat-B']);
  f.unsubscribe();
  await f.store.refresh();
  assert.equal(f.changes.length, 2, 'unsubscribed media owners receive no later invalidations');
  await f.store.close();
});

test('a session replacement drops queued old scopes and obsolete directory responses', async () => {
  const f = await fixture();
  const gate = deferred();
  f.driver.snapshotRead = () => gate.promise;
  const old = f.store.refresh(['chat-B']);
  f.store.refresh(['chat-A']);
  const oldEvent = f.driver.changed;
  await f.store.open('demo');
  f.driver.snapshotRead = async () => {};
  f.store.selectSpace('chat-C');
  await tick();
  f.driver.reads = [];
  await f.store.refresh(['chat-B']);
  oldEvent?.({ kind: 'change', spaceIds: ['chat-C'] });
  gate.resolve();
  await old;
  assert.deepEqual(f.driver.reads, ['snapshot']);
  assert.deepEqual(Object.keys(f.store.getSnapshot().timelines), ['chat-C']);
  await f.store.close();
});

test('a synchronous retirement during invalidation cannot admit a new canonical read', async () => {
  for (const read of ['refresh', 'timeline'] as const) {
    const f = await fixture();
    let closing: Promise<void> | undefined;
    const stop = f.store.subscribeCanonicalChanges(() => {
      closing = f.store.close();
    });
    await (read === 'refresh' ? f.store.refresh(['chat-A']) : f.store.loadTimeline('chat-A'));
    await closing;
    assert.deepEqual(f.driver.reads, []);
    stop();
  }
});
