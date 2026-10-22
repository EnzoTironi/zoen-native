import assert from 'node:assert/strict';
import { test } from 'node:test';
import { homeOwnerKey, orderHomeItems, parseHomePreference } from '../home/preferences.ts';
import {
  deferred,
  failure,
  fixture,
  miniapp,
  ownerA,
  ownerB,
  stored,
} from './home-preferences.fixtures.ts';

test('versioned local preferences reject malformed envelopes and retain bounded IDs only', () => {
  assert.deepEqual(parseHomePreference(stored(['it_one', 'it_one'], ['it_two', 'it_two'])), {
    pinnedIds: ['it_one'],
    orderedIds: ['it_two'],
    hiddenIds: [],
  });
  for (const value of [
    { version: 1, pinnedIds: [], orderedIds: [] },
    { version: 2, pinnedIds: [] },
    { version: 2, pinnedIds: [], orderedIds: [], title: 'Private content' },
    { version: 2, pinnedIds: [], orderedIds: ['../private'] },
    { version: 2, pinnedIds: [], orderedIds: [{ id: 'it_one' }] },
    { version: 2, pinnedIds: [], orderedIds: Array(101).fill('it_one') },
    { version: 2, pinnedIds: 'it_one', orderedIds: [] },
  ])
    assert.deepEqual(parseHomePreference(JSON.stringify(value)), {
      pinnedIds: [],
      orderedIds: [],
      hiddenIds: [],
    });
  assert.deepEqual(parseHomePreference(' '.repeat(32_769)), {
    pinnedIds: [],
    orderedIds: [],
    hiddenIds: [],
  });
  const ids = Array.from({ length: 100 }, (_, index) => `${index}`.padEnd(128, 'a'));
  assert.deepEqual(parseHomePreference(stored(ids, ids)), {
    pinnedIds: ids,
    orderedIds: ids,
    hiddenIds: [],
  });
});

test('all-App and favorite edits share one serialized write without overwriting each other', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  await f.home.activate();
  const firstWrite = deferred<void>();
  const originalSet = f.port.set;
  let writes = 0;
  f.port.set = async (key, value) => {
    if (++writes === 1) await firstWrite.promise;
    await originalSet(key, value);
  };
  const all = f.home.reorderItems(['it_three', 'it_one', 'it_two']);
  const pinOne = f.home.pin('it_one');
  const pinTwo = f.home.pin('it_two');
  const favorites = f.home.reorder(['it_two', 'it_one']);
  await Promise.resolve();
  assert.equal(writes, 1);
  assert.deepEqual(f.home.getSnapshot().orderedIds, []);
  firstWrite.resolve();
  await Promise.all([all, pinOne, pinTwo, favorites]);
  assert.equal(writes, 4);
  assert.equal(
    f.saved.get(homeOwnerKey(ownerA)!),
    stored(['it_two', 'it_one'], ['it_three', 'it_one', 'it_two']),
  );
  await f.home.unpin('it_two');
  assert.deepEqual(f.home.getSnapshot().orderedIds, ['it_three', 'it_one', 'it_two']);
});

test('same-set stale drag orders reject after an earlier queued reorder commits', async () => {
  for (const scope of ['favorites', 'all'] as const) {
    const f = fixture();
    await f.home.activate();
    if (scope === 'favorites') {
      await f.home.pin('it_one');
      await f.home.pin('it_two');
    }
    const reorder = scope === 'favorites' ? f.home.reorder : f.home.reorderItems;
    const pending = deferred<void>();
    const originalSet = f.port.set;
    let writes = 0;
    f.port.set = async (key, value) => {
      ++writes;
      await pending.promise;
      await originalSet(key, value);
    };
    const expected = ['it_one', 'it_two'];
    const first = reorder(['it_two', 'it_one'], ownerA, expected);
    const stale = assert.rejects(reorder(['it_one', 'it_two'], ownerA, expected), failure('save'));
    expected.reverse(); // Queued requests own their snapshots, not the caller's mutable arrays.
    await Promise.resolve();
    assert.equal(writes, 1);
    pending.resolve();
    await Promise.all([first, stale]);
    assert.equal(writes, 1);
    assert.deepEqual(f.home.getSnapshot()[scope === 'favorites' ? 'pinnedIds' : 'orderedIds'], [
      'it_two',
      'it_one',
    ]);
    await reorder(['it_one', 'it_two'], ownerA, ['it_two', 'it_one']);
    assert.equal(writes, 2);
  }
});

test('all-App order retains unavailable slots, appends new canonical Apps and respects current membership', async () => {
  const f = fixture();
  f.saved.set(
    homeOwnerKey(ownerA)!,
    stored(['it_one'], ['not_loaded', 'it_two', 'no_membership', 'it_one']),
  );
  f.items([
    miniapp('it_one'),
    miniapp('it_two'),
    miniapp('no_membership', 'removed'),
    miniapp('it_new'),
  ]);
  await f.home.activate();
  const canonical = f.home.getSnapshot().items;
  assert.deepEqual(
    orderHomeItems(canonical, f.home.getSnapshot().orderedIds).map((item) => item.id),
    ['it_two', 'it_one', 'it_new'],
  );
  await f.home.reorderItems(['it_new', 'it_one', 'it_two']);
  assert.deepEqual(f.home.getSnapshot().orderedIds, [
    'not_loaded',
    'it_new',
    'no_membership',
    'it_one',
    'it_two',
  ]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one']);
  assert.equal(f.home.getSnapshot().items, canonical);
  const saved = f.saved.get(homeOwnerKey(ownerA)!);
  f.spaces([]); // Revocation is authoritative even before the canonical refresh returns.
  await assert.rejects(f.home.reorderItems(['it_two', 'it_new', 'it_one']), failure('save'));
  await f.home.reorderItems([]);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), saved);
});

test('all-App orders reject invalid selections, stale item sets, and edits before restore', async () => {
  const f = fixture();
  const restore = deferred<string | null>();
  f.port.get = () => restore.promise;
  const activation = f.home.activate();
  await assert.rejects(f.home.reorderItems(['it_two', 'it_one']), failure('save'));
  restore.resolve(null);
  await activation;
  for (const order of [
    [],
    ['it_one'],
    ['it_one', 'it_one'],
    ['it_two', 'missing'],
    ['it_one', '../private'],
    Array(101).fill('it_one') as string[],
  ])
    await assert.rejects(f.home.reorderItems(order), failure('save'));
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_new')]);
  await f.home.refresh();
  await assert.rejects(f.home.reorderItems(['it_two', 'it_one']), failure('save'));
  assert.equal(f.saved.size, 0);
});

test('failed all-App writes preserve confirmed favorites/order and subsequent edits recover', async () => {
  const f = fixture();
  await f.home.activate();
  await f.home.pin('it_one');
  const originalSet = f.port.set;
  f.port.set = async () => {
    throw new Error('Storage unavailable');
  };
  await assert.rejects(f.home.reorderItems(['it_two', 'it_one']), failure('save'));
  assert.deepEqual(f.home.getSnapshot().orderedIds, []);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_one']));
  f.port.set = originalSet;
  await f.home.reorderItems(['it_two', 'it_one']);
  assert.equal(f.home.getSnapshot().error, null);
});

test('all-App captured drag actions and saved orders stay isolated by account, device and mode', async () => {
  for (const nextOwner of [
    ownerB,
    { ...ownerA, deviceId: 'other' },
    { ...ownerA, mode: 'demo' as const },
  ]) {
    const f = fixture();
    await f.home.activate();
    await f.home.reorderItems(['it_two', 'it_one']);
    const saved = f.saved.get(homeOwnerKey(ownerA)!);
    f.owner(nextOwner);
    await f.home.refresh();
    await assert.rejects(
      f.home.reorderItems(['it_two', 'it_one'], ownerA),
      failure('accountChanged'),
    );
    assert.deepEqual(f.home.getSnapshot().orderedIds, []);
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), saved);
    assert.equal(f.saved.has(homeOwnerKey(nextOwner)!), false);
  }
});

test('in-flight all-App writes finish only on the old owner key and erasure drains the complete preference', async () => {
  for (const erase of [false, true]) {
    const f = fixture();
    await f.home.activate();
    await f.home.pin('it_one');
    const pending = deferred<void>();
    const originalSet = f.port.set;
    f.port.set = async (key, value) => {
      await pending.promise;
      await originalSet(key, value);
    };
    const reorder = assert.rejects(
      f.home.reorderItems(['it_two', 'it_one']),
      failure('accountChanged'),
    );
    await Promise.resolve();
    f.owner(erase ? null : ownerB);
    await f.home.refresh();
    const cleanup = erase ? f.home.eraseDevice() : Promise.resolve();
    pending.resolve();
    await Promise.all([reorder, cleanup]);
    assert.deepEqual(f.home.getSnapshot().orderedIds, []);
    assert.equal(
      f.saved.get(homeOwnerKey(ownerA)!),
      erase ? '' : stored(['it_one'], ['it_two', 'it_one']),
    );
    assert.equal(f.saved.has(homeOwnerKey(ownerB)!), false);
  }
});

test('demo all-App order is volatile and disappears on reactivation', async () => {
  const f = fixture();
  f.owner({ ...ownerA, mode: 'demo' });
  await f.home.activate();
  await f.home.reorderItems(['it_two', 'it_one']);
  assert.deepEqual(f.home.getSnapshot().orderedIds, ['it_two', 'it_one']);
  assert.equal(f.saved.size, 0);
  f.home.dispose();
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().orderedIds, []);
});
