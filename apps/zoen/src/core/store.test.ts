import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
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
}

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
