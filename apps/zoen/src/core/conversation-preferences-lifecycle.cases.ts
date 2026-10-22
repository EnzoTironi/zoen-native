import assert from 'node:assert/strict';
import { test } from 'node:test';
import { conversationOwnerKey } from '../miniapps/conversation-preferences.ts';
import {
  deferred,
  failure,
  fixture,
  ids,
  miniapp,
  ownerA,
  ownerB,
  preference,
} from './conversation-preferences.fixtures.ts';

test('owner changes fence late restores, delayed menu callbacks and in-flight writes, including same-owner return', async () => {
  const f = fixture();
  const restore = deferred<string | null>();
  f.port.get = (key) =>
    key === conversationOwnerKey(ownerA) ? restore.promise : Promise.resolve(null);
  const old = f.cards.activate();
  const oldOwner = f.cards.getSnapshot().owner;
  f.owner(ownerB);
  await f.cards.refresh();
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await old;
  assert.deepEqual(f.cards.getSnapshot().owner, ownerB);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
  await assert.rejects(
    f.cards.setPinned('shared', 'it_one', false, oldOwner),
    failure('accountChanged'),
  );

  f.port.get = async (key) => f.saved.get(key) ?? null;
  const write = deferred<void>();
  const original = f.port.set;
  f.port.set = async (key, value) => {
    await write.promise;
    await original(key, value);
  };
  const oldWrite = f.cards.setPinned('shared', 'it_one', false);
  const refusal = assert.rejects(oldWrite, failure('accountChanged'));
  await Promise.resolve();
  f.owner(null);
  await f.cards.refresh();
  f.owner(ownerB);
  const returning = f.cards.refresh();
  assert.equal(f.cards.getSnapshot().loading, true);
  assert.deepEqual(f.cards.cards('shared').all, []);
  write.resolve();
  await Promise.all([refusal, returning]);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two']);
  assert.equal(f.cards.getSnapshot().error, null);
  assert.deepEqual(JSON.parse(f.saved.get(conversationOwnerKey(ownerB)!)!), {
    shared: preference(['it_one']),
  });
});

test('a slow previous-owner write does not block restoration for an unrelated owner', async () => {
  const f = fixture();
  await f.cards.activate();
  const pending = deferred<void>();
  f.port.set = async (key, value) => {
    await pending.promise;
    f.saved.set(key, value);
  };
  const old = assert.rejects(
    f.cards.setPinned('shared', 'it_one', false),
    failure('accountChanged'),
  );
  await Promise.resolve();
  f.saved.set(conversationOwnerKey(ownerB)!, JSON.stringify({ shared: preference(['it_two']) }));
  f.owner(ownerB);
  await f.cards.refresh();
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one']);
  pending.resolve();
  await old;
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one']);
});

test('account, device and demo scopes reject stale callbacks even when they share the same canonical IDs', async () => {
  const f = fixture();
  await f.cards.activate();
  for (const owner of [
    { ...ownerA, deviceId: 'phone_other' },
    { ...ownerA, mode: 'demo' as const },
    ownerB,
  ]) {
    const captured = f.cards.getSnapshot().owner;
    f.owner(owner);
    assert.deepEqual(f.cards.cards('shared').all, []);
    await f.cards.refresh();
    await assert.rejects(
      f.cards.setPinned('shared', 'it_one', false, captured),
      failure('accountChanged'),
    );
    await assert.rejects(
      f.cards.reorder('shared', ['it_two', 'it_one'], captured),
      failure('accountChanged'),
    );
  }
  assert.equal(f.saved.size, 0);
});

test('erase drains pending writes, fences late restores and clears only the captured device preference key', async () => {
  const f = fixture();
  f.saved.set(conversationOwnerKey(ownerB)!, JSON.stringify({ shared: preference(['it_two']) }));
  await f.cards.activate();
  const pending = deferred<void>();
  const original = f.port.set;
  let first = true;
  f.port.set = async (key, value) => {
    if (first) {
      first = false;
      await pending.promise;
    }
    await original(key, value);
  };
  const hide = assert.rejects(
    f.cards.setPinned('shared', 'it_one', false),
    failure('accountChanged'),
  );
  await Promise.resolve();
  f.owner(null);
  await f.cards.refresh();
  f.cards.dispose();
  const erase = f.cards.eraseDevice();
  pending.resolve();
  await Promise.all([hide, erase]);
  assert.equal(f.saved.get(conversationOwnerKey(ownerA)!), '');
  assert.deepEqual(JSON.parse(f.saved.get(conversationOwnerKey(ownerB)!)!), {
    shared: preference(['it_two']),
  });
  assert.deepEqual(f.cards.cards('shared').all, []);

  const late = fixture();
  const restore = deferred<string | null>();
  late.port.get = () => restore.promise;
  const active = late.cards.activate();
  late.owner(null);
  await late.cards.eraseDevice();
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await active;
  assert.deepEqual(Object.keys(late.cards.getSnapshot().preferences), []);
  assert.equal(late.saved.get(conversationOwnerKey(ownerA)!), '');
});

test('failed erasure preserves cleanup ownership and blocks every new write until erasure retry succeeds', async () => {
  const f = fixture();
  await f.cards.activate();
  await f.cards.setPinned('shared', 'it_one', false);
  f.owner(null);
  await f.cards.refresh();
  const original = f.port.set;
  f.port.set = async () => {
    throw new Error('Cannot clear preferences');
  };
  await assert.rejects(f.cards.eraseDevice(), /Cannot clear preferences/);
  await assert.rejects(f.cards.setPinned('shared', 'it_two', false), failure('accountChanged'));
  f.port.set = original;
  await f.cards.eraseDevice();
  assert.equal(f.saved.get(conversationOwnerKey(ownerA)!), '');
});

test('demo choices remain volatile and disposed pending work cannot publish after remount', async () => {
  const f = fixture();
  f.owner({ ...ownerA, mode: 'demo' });
  let reads = 0;
  f.port.get = async () => {
    ++reads;
    return JSON.stringify({ shared: preference(['it_one']) });
  };
  await f.cards.activate();
  await f.cards.setPinned('shared', 'it_one', false);
  await f.cards.reorder('shared', ['it_two']);
  assert.equal(reads, 0);
  assert.equal(f.saved.size, 0);
  f.cards.dispose();
  await f.cards.activate();
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);

  const late = fixture();
  const restore = deferred<string | null>();
  late.port.get = () => restore.promise;
  const old = late.cards.activate();
  late.cards.dispose();
  late.port.get = async () => JSON.stringify({ shared: preference(['it_two']) });
  await late.cards.activate();
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await old;
  assert.deepEqual(ids(late.cards.cards('shared').visible), ['it_one']);
});

test('preference map and ID limits fail safely without truncating a committed chat selection', async () => {
  const f = fixture();
  f.saved.set(
    conversationOwnerKey(ownerA)!,
    JSON.stringify(
      Object.fromEntries(
        Array.from({ length: 100 }, (_, i) => [`space_${i}`, preference(['it_hidden'])]),
      ),
    ),
  );
  await f.cards.activate();
  await assert.rejects(f.cards.setPinned('shared', 'it_one', false), failure('save'));
  assert.equal(Object.keys(f.cards.getSnapshot().preferences).length, 100);
  assert.equal(f.cards.getSnapshot().preferences.shared, undefined);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
});

test('typing-only refreshes preserve snapshot identity while canonical and access changes notify subscribers', async () => {
  const f = fixture();
  await f.cards.activate();
  const initial = f.cards.getSnapshot();
  let notifications = 0;
  f.cards.subscribe(() => {
    ++notifications;
  });
  // The source may allocate equivalent membership sets on every ClientStore update.
  f.source.spaces = () => new Set(['shared', 'second']);
  await Promise.all([f.cards.refresh(), f.cards.refresh(), f.cards.refresh()]);
  assert.equal(notifications, 0);
  assert.equal(f.cards.getSnapshot(), initial);
  f.items([...f.source.items(), miniapp('it_new')]);
  await f.cards.refresh();
  assert.equal(notifications, 1);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two', 'it_new']);
  f.source.spaces = () => new Set(['second']);
  await f.cards.refresh();
  assert.equal(notifications, 2);
  assert.deepEqual(f.cards.cards('shared').all, []);
});
