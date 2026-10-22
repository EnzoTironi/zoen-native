import assert from 'node:assert/strict';
import { test } from 'node:test';
import { homeOwnerKey } from '../home/preferences.ts';
import {
  deferred,
  failure,
  fixture,
  miniapp,
  ownerA,
  ownerB,
  projection,
  stored,
} from './home-preferences.fixtures.ts';

test('denied shortcut writes preserve the confirmed projection and the next queued mutation can recover', async () => {
  for (const operation of ['hide', 'restore', 'restoreAll', 'pin'] as const) {
    const f = fixture();
    const hidden = operation === 'hide' ? [] : ['it_one'];
    f.saved.set(homeOwnerKey(ownerA)!, stored([], ['it_two', 'it_one'], hidden));
    await f.home.activate();
    const before = projection(f.home);
    const originalSet = f.port.set;
    f.port.set = async () => {
      throw new Error('Storage denied');
    };
    const attempt = operation === 'restoreAll' ? f.home.restoreAll() : f.home[operation]('it_one');
    await assert.rejects(attempt, failure('save'));
    assert.deepEqual(projection(f.home), before);
    assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored([], ['it_two', 'it_one'], hidden));
    f.port.set = originalSet;
    await f.home.pin('it_one');
    assert.deepEqual(projection(f.home).visible, ['it_two', 'it_one']);
    assert.equal(f.home.getSnapshot().error, null);
  }
});

test('shortcut mutations require completed preference restore and current artifact authorization at admission', async () => {
  const f = fixture();
  const pending = deferred<string | null>();
  f.port.get = () => pending.promise;
  const activation = f.home.activate();
  for (const action of [
    () => f.home.hide('it_one'),
    () => f.home.restore('it_one'),
    () => f.home.restoreAll(),
  ])
    await assert.rejects(action(), failure('save'));
  pending.resolve(null);
  await activation;
  for (const action of ['hide', 'restore', 'pin'] as const) {
    await assert.rejects(f.home[action]('../private'), failure('save'));
    await assert.rejects(f.home[action]('unavailable'), failure('save'));
    const queued = assert.rejects(f.home[action]('it_one'), failure('save'));
    f.spaces([]);
    await queued;
    f.spaces(['shared']);
  }
  assert.equal(f.saved.size, 0);
});

test('captured hide, restore, restore-all and pin actions cannot cross account, device or mode ownership', async () => {
  for (const next of [
    ownerB,
    { ...ownerA, deviceId: 'other' },
    { ...ownerA, mode: 'demo' as const },
  ]) {
    const f = fixture();
    await f.home.activate();
    await f.home.hide('it_one');
    const saved = f.saved.get(homeOwnerKey(ownerA)!);
    f.owner(next);
    await f.home.refresh();
    for (const action of ['hide', 'restore', 'pin'] as const)
      await assert.rejects(f.home[action]('it_one', ownerA), failure('accountChanged'));
    await assert.rejects(f.home.restoreAll(ownerA), failure('accountChanged'));
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), saved);
    assert.equal(f.saved.has(homeOwnerKey(next)!), false);
    assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
  }
});

test('an accepted hidden preference finishes only for its original owner, and device erasure drains then clears it', async () => {
  for (const erase of [false, true]) {
    const f = fixture();
    await f.home.activate();
    const pending = deferred<void>();
    const originalSet = f.port.set;
    f.port.set = async (key, value) => {
      await pending.promise;
      await originalSet(key, value);
    };
    const hidden = assert.rejects(f.home.hide('it_one'), failure('accountChanged'));
    const queued = assert.rejects(f.home.pin('it_two'), failure('accountChanged'));
    await Promise.resolve();
    f.owner(erase ? null : ownerB);
    await f.home.refresh();
    const cleanup = erase ? f.home.eraseDevice() : Promise.resolve();
    pending.resolve();
    await Promise.all([hidden, queued, cleanup]);
    assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), erase ? '' : stored([], [], ['it_one']));
    assert.equal(f.saved.has(homeOwnerKey(ownerB)!), false);
  }
});

test('demo hidden shortcuts stay volatile and disposed queued edits never revive after reactivation', async () => {
  const f = fixture();
  f.owner({ ...ownerA, mode: 'demo' });
  await f.home.activate();
  await f.home.hide('it_one');
  assert.deepEqual(projection(f.home).hidden, ['it_one']);
  assert.equal(f.saved.size, 0);
  const stale = assert.rejects(f.home.restore('it_one'), failure('accountChanged'));
  f.home.dispose();
  await f.home.activate();
  await stale;
  assert.deepEqual(projection(f.home).hidden, []);
});

test('capacity failures preserve both hidden and favorite fields instead of partially restoring an App', async () => {
  for (const full of ['hidden', 'pinned'] as const) {
    const f = fixture();
    const ids = Array.from({ length: 100 }, (_, index) => `unavailable_${index}`);
    const original = full === 'hidden' ? stored([], [], ids) : stored(ids, [], ['it_one']);
    f.saved.set(homeOwnerKey(ownerA)!, original);
    await f.home.activate();
    const before = projection(f.home);
    let writes = 0;
    f.port.set = async () => {
      ++writes;
    };
    await assert.rejects(
      full === 'hidden' ? f.home.hide('it_one') : f.home.pin('it_one'),
      failure('save'),
    );
    assert.equal(writes, 0);
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), original);
    assert.deepEqual(projection(f.home), before);
    assert.deepEqual(f.home.getSnapshot().pinnedIds, full === 'pinned' ? ids : []);
  }
});

test('a failed queued restore does not invalidate a drag against the unchanged confirmed hidden layout', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  f.saved.set(homeOwnerKey(ownerA)!, stored([], [], ['it_two']));
  await f.home.activate();
  const originalSet = f.port.set;
  let writes = 0;
  f.port.set = async (key, value) => {
    if (++writes === 1) throw new Error('Write denied');
    await originalSet(key, value);
  };
  const restore = assert.rejects(f.home.restore('it_two'), failure('save'));
  const drag = f.home.reorderItems(['it_three', 'it_one'], ownerA, ['it_one', 'it_three']);
  await Promise.all([restore, drag]);
  assert.deepEqual(projection(f.home), {
    visible: ['it_three', 'it_one'],
    hidden: ['it_two'],
    all: ['it_three', 'it_two', 'it_one'],
  });
  assert.equal(f.home.getSnapshot().error, null);
});
