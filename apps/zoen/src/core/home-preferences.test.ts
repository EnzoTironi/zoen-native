import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  HomeController,
  HomeFailure,
  homeOwnerKey,
  parseHomePins,
  type HomeOwner,
  type HomePreferencesPort,
} from '../home/preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';

const ownerA: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone_a' };
const ownerB: HomeOwner = { mode: 'account', identityId: 'bruno', deviceId: 'phone_b' };
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function miniapp(id: string, space = 'shared'): ItemDetail {
  return {
    id,
    space_id: space,
    title: `User title ${id}`,
    app: { app_id: 'poll', snapshot_json: '{"private":"canonical only"}' },
  } as ItemDetail;
}
function fixture() {
  let owner: HomeOwner | null = ownerA;
  let spaces = new Set(['shared']);
  let items = [miniapp('it_one'), miniapp('it_two')];
  const saved = new Map<string, string>();
  const port: HomePreferencesPort = {
    get: async (key) => saved.get(key) ?? null,
    set: async (key, value) => {
      saved.set(key, value);
    },
  };
  const source = {
    owner: () => owner,
    spaces: () => spaces,
    items: async () => items,
  };
  const home = new HomeController(source, port);
  return {
    home,
    port,
    source,
    saved,
    owner: (value: HomeOwner | null) => {
      owner = value;
    },
    spaces: (value: string[]) => {
      spaces = new Set(value);
    },
    items: (value: ItemDetail[]) => {
      items = value;
    },
  };
}

test('pins persist IDs only, stay explicitly empty by default, and do not duplicate canonical miniapp state', async () => {
  const f = fixture();
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.home.getSnapshot().items[0]?.app?.snapshot_json, '{"private":"canonical only"}');
  await f.home.pin('it_two');
  await f.home.pin('it_one');
  await f.home.pin('it_two');
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two', 'it_one']);
  assert.deepEqual([...f.saved.values()], ['["it_two","it_one"]']);
  await f.home.unpin('it_two');
  await f.home.unpin('it_one');
  await f.home.refresh();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.home.getSnapshot().items.length, 2);
});

test('pin parsing is bounded, deduplicates IDs, and rejects content or malformed structures', () => {
  assert.deepEqual(parseHomePins('["it_one","it_one","it_two"]'), ['it_one', 'it_two']);
  for (const value of [
    null,
    '',
    '{}',
    'null',
    '["../private"]',
    '[{"id":"it_one","title":"secret"}]',
    JSON.stringify(Array(101).fill('it_one')),
    ' '.repeat(16_385),
  ])
    assert.deepEqual(parseHomePins(value), []);
  assert.notEqual(homeOwnerKey(ownerA), homeOwnerKey({ ...ownerA, deviceId: 'other' }));
  assert.notEqual(homeOwnerKey(ownerA), homeOwnerKey({ ...ownerA, mode: 'demo' }));
});

test('only canonical miniapps in currently accessible member spaces appear, including before a slow refresh completes', async () => {
  const f = fixture();
  f.items([
    miniapp('it_one'),
    miniapp('it_one'),
    miniapp('it_other', 'removed'),
    // Snapshot has no Personal-space authorization DTO; an unknown cached scope is excluded.
    miniapp('it_personal', 'personal_not_in_snapshot'),
    { ...miniapp('it_note'), app: null },
  ]);
  await f.home.activate();
  assert.deepEqual(
    f.home.getSnapshot().items.map((item) => item.id),
    ['it_one'],
  );
  await f.home.pin('it_one');
  const slow = deferred<ItemDetail[]>();
  f.source.items = () => slow.promise;
  f.spaces([]);
  const refresh = f.home.refresh();
  assert.deepEqual(f.home.getSnapshot().items, []);
  await assert.rejects(
    f.home.pin('it_one'),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  slow.resolve([miniapp('it_one')]);
  await refresh;
  assert.deepEqual(f.home.getSnapshot().items, []);
  // A local ID is no authorization: the unavailable miniapp itself is never cached for display.
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one']);
});

test('late account-A item and preference restores cannot replace account-B content or pins', async () => {
  const f = fixture();
  const oldPins = deferred<string | null>();
  const oldItems = deferred<ItemDetail[]>();
  f.port.get = (key) =>
    key === homeOwnerKey(ownerA) ? oldPins.promise : Promise.resolve('["it_two"]');
  f.source.items = () => oldItems.promise;
  const old = f.home.activate();
  f.owner(ownerB);
  f.source.items = async () => [miniapp('it_two')];
  await f.home.refresh();
  oldPins.resolve('["it_one"]');
  oldItems.resolve([miniapp('it_one')]);
  await old;
  assert.deepEqual(f.home.getSnapshot().owner, ownerB);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  assert.deepEqual(
    f.home.getSnapshot().items.map((item) => item.id),
    ['it_two'],
  );
});

test('pin changes and reordering cannot race ahead of a pending restore and overwrite an existing selection', async () => {
  const f = fixture();
  const pins = deferred<string | null>();
  f.port.get = () => pins.promise;
  const loading = f.home.activate();
  await Promise.resolve();
  await assert.rejects(
    f.home.pin('it_one'),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  await assert.rejects(
    f.home.reorder([]),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  assert.equal(f.saved.size, 0);
  pins.resolve('["it_two"]');
  await loading;
  await f.home.pin('it_one');
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two', 'it_one']);
});

test('a canonical change during a pending read coalesces into a fresh read without publishing the stale result', async () => {
  const f = fixture();
  const slow = deferred<ItemDetail[]>();
  let reads = 0;
  f.source.items = () => (++reads === 1 ? slow.promise : Promise.resolve([miniapp('it_two')]));
  const published: string[][] = [];
  f.home.subscribe(() => {
    published.push(f.home.getSnapshot().items.map((item) => item.id));
  });
  const first = f.home.activate();
  const newer = f.home.refresh();
  slow.resolve([miniapp('it_one')]);
  await Promise.all([first, newer]);
  assert.equal(reads, 2);
  assert.equal(
    published.some((ids) => ids.includes('it_one')),
    false,
  );
  assert.deepEqual(
    f.home.getSnapshot().items.map((item) => item.id),
    ['it_two'],
  );
});

test('serialized pin/remove operations preserve user order and a failed write preserves the confirmed selection', async () => {
  const f = fixture();
  await f.home.activate();
  const firstWrite = deferred<void>();
  let writes = 0;
  f.port.set = async (key, value) => {
    if (++writes === 1) await firstWrite.promise;
    f.saved.set(key, value);
  };
  const first = f.home.pin('it_one');
  const second = f.home.pin('it_two');
  const third = f.home.unpin('it_one');
  await Promise.resolve();
  assert.equal(writes, 1);
  firstWrite.resolve();
  await Promise.all([first, second, third]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  f.port.set = async () => {
    throw new Error('Storage is unavailable');
  };
  await assert.rejects(
    f.home.unpin('it_two'),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'save',
  );
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  const cause = f.home.getSnapshot().error?.cause;
  assert.equal(cause instanceof Error && cause.message, 'Storage is unavailable');
});

test('an old-account write cannot publish into a new account, while both device-scoped selections remain separate', async () => {
  const f = fixture();
  await f.home.activate();
  const write = deferred<void>();
  f.port.set = async (key, value) => {
    await write.promise;
    f.saved.set(key, value);
  };
  const pin = f.home.pin('it_one');
  const refusal = assert.rejects(
    pin,
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  await Promise.resolve();
  f.owner(ownerB);
  await f.home.refresh();
  write.resolve();
  await refusal;
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.home.getSnapshot().error, null);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_one"]');
  await f.home.pin('it_two');
  assert.equal(f.saved.get(homeOwnerKey(ownerB)!), '["it_two"]');
});

test('a delayed menu from a previous account cannot pin an item shared with the current account', async () => {
  const f = fixture();
  await f.home.activate();
  const menuOwner = f.home.getSnapshot().owner;
  f.owner(ownerB);
  await f.home.refresh();
  assert.equal(f.home.getSnapshot().items[0]?.id, 'it_one');
  await assert.rejects(
    f.home.pin('it_one', menuOwner),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.saved.size, 0);
});

test('device erasure drains an in-flight pin writer, clears the captured owner, and fences late restores after unmount', async () => {
  const f = fixture();
  f.saved.set(homeOwnerKey(ownerB)!, '["it_two"]');
  await f.home.activate();
  const write = deferred<void>();
  const originalSet = f.port.set;
  let first = true;
  f.port.set = async (key, value) => {
    if (first) {
      first = false;
      await write.promise;
    }
    await originalSet(key, value);
  };
  const pin = f.home.pin('it_one');
  const refusal = assert.rejects(
    pin,
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  await Promise.resolve();
  f.owner(null);
  await f.home.refresh();
  f.home.dispose();
  const erase = f.home.eraseDevice();
  write.resolve();
  await Promise.all([refusal, erase]);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '');
  assert.equal(f.saved.get(homeOwnerKey(ownerB)!), '["it_two"]');
  assert.deepEqual(f.home.getSnapshot().items, []);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);

  const late = fixture();
  const restore = deferred<string | null>();
  late.port.get = () => restore.promise;
  const active = late.home.activate();
  late.owner(null);
  await late.home.eraseDevice();
  restore.resolve('["it_one"]');
  await active;
  assert.equal(late.saved.get(homeOwnerKey(ownerA)!), '');
  assert.deepEqual(late.home.getSnapshot().pinnedIds, []);
});

test('failed erasure retains the original cleanup owner for retry and blocks new writes', async () => {
  const f = fixture();
  await f.home.activate();
  await f.home.pin('it_one');
  f.owner(null);
  await f.home.refresh();
  const originalSet = f.port.set;
  f.port.set = async () => {
    throw new Error('Cannot clear preferences');
  };
  await assert.rejects(f.home.eraseDevice(), /Cannot clear preferences/);
  await assert.rejects(
    f.home.pin('it_two'),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  f.port.set = originalSet;
  await f.home.eraseDevice();
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '');
  assert.deepEqual(f.home.getSnapshot().items, []);
});

test('demo pins remain volatile and effect remounts can reload without publishing disposed requests', async () => {
  const f = fixture();
  f.owner({ ...ownerA, mode: 'demo' });
  let reads = 0;
  f.port.get = async () => {
    ++reads;
    return '["it_two"]';
  };
  await f.home.activate();
  await f.home.pin('it_one');
  assert.equal(reads, 0);
  assert.equal(f.saved.size, 0);
  f.home.dispose();
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.home.getSnapshot().items.length, 2);
});

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
    '["not_loaded","it_two","no_membership","it_one"]',
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
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_one","it_two"]');
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
  assert.deepEqual(writes, ['["it_three","it_one","it_two"]']);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one', 'it_two', 'it_three']);
  firstWrite.resolve();
  await Promise.all([first, second, remove]);
  assert.deepEqual(writes, [
    '["it_three","it_one","it_two"]',
    '["it_two","it_three","it_one"]',
    '["it_two","it_one"]',
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
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_two","it_one"]');
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
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_one","it_two"]');
  const cause = f.home.getSnapshot().error?.cause;
  assert.equal(cause instanceof Error && cause.message, 'Storage is unavailable');
  await f.home.reorder(['it_two', 'it_one']);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two', 'it_one']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_two","it_one"]');
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
    assert.deepEqual([...f.saved.entries()], [[homeOwnerKey(ownerA), '["it_one","it_two"]']]);
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
    assert.equal(f.saved.get(homeOwnerKey(ownerA)!), erase ? '' : '["it_two","it_one"]');
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
