import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, Driver, empty, entry, unreadSnapshot } from './store.fixtures.ts';
import { ClientStore } from './store.ts';
import type { TimelinePage } from './types.ts';

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
