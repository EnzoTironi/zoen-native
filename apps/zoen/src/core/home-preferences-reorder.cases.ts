import assert from 'node:assert/strict';
import { test } from 'node:test';
import { HomeFailure, homeOwnerKey, orderHomeItems, type HomeOwner } from '../home/preferences.ts';
import { deferred, fixture, miniapp, ownerA, ownerB, stored } from './home-preferences.fixtures.ts';

test('favorite reordering persists only the exact visible ID order and retains unavailable pins in their slots', async () => {
  const f = fixture();
  f.saved.set(homeOwnerKey(ownerA)!, '["not_loaded","it_one","no_membership","it_two"]');
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('no_membership', 'removed')]);
  await f.home.activate();
  await f.home.reorder(['it_two', 'it_one']);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, [
    'not_loaded',
    'it_two',
    'no_membership',
    'it_one',
  ]);
  assert.equal(
    f.saved.get(homeOwnerKey(ownerA)!),
    stored(['not_loaded', 'it_two', 'no_membership', 'it_one']),
  );
  f.home.dispose();
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, [
    'not_loaded',
    'it_two',
    'no_membership',
    'it_one',
  ]);
});

test('reordering rejects incomplete, duplicate, unpinned, inaccessible and content-bearing IDs without writing', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  await f.home.activate();
  await f.home.pin('it_one');
  await f.home.pin('it_two');
  let writes = 0;
  f.port.set = async () => {
    ++writes;
  };
  for (const order of [
    [],
    ['it_one'],
    ['it_one', 'it_one'],
    ['it_one', 'it_three'],
    ['it_one', '../private'],
    ['it_one', { id: 'it_two', title: 'Private content' }] as unknown as string[],
    Array(101).fill('it_one') as string[],
  ])
    await assert.rejects(
      f.home.reorder(order),
      (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
    );
  await f.home.reorder(['it_one', 'it_two']);
  // A membership change is authoritative before an item refresh has completed.
  f.spaces([]);
  await assert.rejects(
    f.home.reorder(['it_two', 'it_one']),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  await f.home.reorder([]);
  assert.equal(writes, 0);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one', 'it_two']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_one', 'it_two']));
});

test('queued reorders copy the requested IDs and serialize with pin removals using the last committed order', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  await f.home.activate();
  for (const id of ['it_one', 'it_two', 'it_three']) await f.home.pin(id);
  const firstWrite = deferred<void>();
  const writes: string[] = [];
  f.port.set = async (key, value) => {
    writes.push(value);
    if (writes.length === 1) await firstWrite.promise;
    f.saved.set(key, value);
  };
  const first = f.home.reorder(['it_three', 'it_one', 'it_two']);
  const requested = ['it_two', 'it_three', 'it_one'];
  const second = f.home.reorder(requested);
  requested.fill('it_three');
  const remove = f.home.unpin('it_three');
  await Promise.resolve();
  assert.deepEqual(writes, [stored(['it_three', 'it_one', 'it_two'])]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one', 'it_two', 'it_three']);
  firstWrite.resolve();
  await Promise.all([first, second, remove]);
  assert.deepEqual(writes, [
    stored(['it_three', 'it_one', 'it_two']),
    stored(['it_two', 'it_three', 'it_one']),
    stored(['it_two', 'it_one']),
  ]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two', 'it_one']);
});

test('queued selection changes reject a stale reorder instead of restoring removed pins or dropping new ones', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  await f.home.activate();
  await f.home.pin('it_one');
  await f.home.pin('it_two');
  const pin = f.home.pin('it_three');
  const oldSelection = assert.rejects(
    f.home.reorder(['it_two', 'it_one']),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  await Promise.all([pin, oldSelection]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one', 'it_two', 'it_three']);
  const remove = f.home.unpin('it_three');
  const removedSelection = assert.rejects(
    f.home.reorder(['it_three', 'it_two', 'it_one']),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  await Promise.all([remove, removedSelection]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one', 'it_two']);
  await f.home.reorder(['it_two', 'it_one']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_two', 'it_one']));
});

test('a failed reorder retains the committed order and the next queued reorder can still persist', async () => {
  const f = fixture();
  await f.home.activate();
  await f.home.pin('it_one');
  await f.home.pin('it_two');
  const originalSet = f.port.set;
  let attempts = 0;
  f.port.set = async (key, value) => {
    if (++attempts === 1) throw new Error('Storage is unavailable');
    await originalSet(key, value);
  };
  const failed = assert.rejects(
    f.home.reorder(['it_two', 'it_one']),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  await failed;
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one', 'it_two']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_one', 'it_two']));
  const cause = f.home.getSnapshot().error?.cause;
  assert.equal(cause instanceof Error && cause.message, 'Storage is unavailable');
  await f.home.reorder(['it_two', 'it_one']);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two', 'it_one']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_two', 'it_one']));
  assert.equal(f.home.getSnapshot().error, null);
});

test('reordering rejects captured actions after an account, device or mode change', async () => {
  for (const nextOwner of [ownerB, { ...ownerA, deviceId: 'other' }, { ...ownerA, mode: 'demo' }]) {
    const f = fixture();
    await f.home.activate();
    await f.home.pin('it_one');
    await f.home.pin('it_two');
    const oldOwner = f.home.getSnapshot().owner;
    f.owner(nextOwner as HomeOwner);
    await f.home.refresh();
    await assert.rejects(
      f.home.reorder(['it_two', 'it_one'], oldOwner),
      (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
    );
    assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
    assert.deepEqual(
      [...f.saved.entries()],
      [[homeOwnerKey(ownerA), stored(['it_one', 'it_two'])]],
    );
  }
});

test('an in-flight reorder cannot publish into a new owner and device erasure drains it before clearing', async () => {
  for (const erase of [false, true]) {
    const f = fixture();
    await f.home.activate();
    await f.home.pin('it_one');
    await f.home.pin('it_two');
    const write = deferred<void>();
    const originalSet = f.port.set;
    f.port.set = async (key, value) => {
      await write.promise;
      await originalSet(key, value);
    };
    const reorder = assert.rejects(
      f.home.reorder(['it_two', 'it_one']),
      (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
    );
    await Promise.resolve();
    f.owner(erase ? null : ownerB);
    await f.home.refresh();
    const erasure = erase ? f.home.eraseDevice() : Promise.resolve();
    write.resolve();
    await Promise.all([reorder, erasure]);
    assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
    assert.equal(f.home.getSnapshot().error, null);
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), erase ? '' : stored(['it_two', 'it_one']));
    assert.equal(f.saved.has(homeOwnerKey(ownerB)!), false);
  }
});

test('demo reordering stays volatile and disposed queued work cannot revive after activation', async () => {
  const f = fixture();
  f.owner({ ...ownerA, mode: 'demo' });
  await f.home.activate();
  await f.home.pin('it_one');
  await f.home.pin('it_two');
  await f.home.reorder(['it_two', 'it_one']);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two', 'it_one']);
  assert.equal(f.saved.size, 0);
  const old = assert.rejects(
    f.home.reorder(['it_one', 'it_two']),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  f.home.dispose();
  await f.home.activate();
  await old;
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.saved.size, 0);
});

test('legacy favorites migrate in the same device key only when a real order edit commits', async () => {
  const f = fixture();
  const key = homeOwnerKey(ownerA)!;
  f.saved.set(key, '["it_two"]');
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  assert.deepEqual(f.home.getSnapshot().orderedIds, []);
  assert.equal(f.saved.get(key), '["it_two"]');
  await f.home.reorderItems(['it_two', 'it_one']);
  assert.deepEqual([...f.saved], [[key, stored(['it_two'], ['it_two', 'it_one'])]]);
  f.home.dispose();
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  assert.deepEqual(f.home.getSnapshot().orderedIds, ['it_two', 'it_one']);
  // This source also feeds conversation defaults; it must retain canonical order.
  assert.deepEqual(
    f.home.getSnapshot().items.map((item) => item.id),
    ['it_one', 'it_two'],
  );
  assert.deepEqual(
    orderHomeItems(f.home.getSnapshot().items, f.home.getSnapshot().orderedIds).map(
      (item) => item.id,
    ),
    ['it_two', 'it_one'],
  );
});
