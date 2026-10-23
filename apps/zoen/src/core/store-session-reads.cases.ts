import assert from 'node:assert/strict';
import { test } from 'node:test';
import { completeProviderSession } from '../auth/complete-session.ts';
import { deferred, Driver, empty, entry, tick } from './store.fixtures.ts';
import { ClientStore } from './store.ts';
import type { Snapshot, TimelinePage } from './types.ts';

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
