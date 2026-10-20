import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  HomeController,
  HomeFailure,
  homeOwnerKey,
  parseHomePins,
  parseHomePreference,
  orderHomeItems,
  type HomeOwner,
  type HomePreferencesPort,
} from '../home/preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';

const ownerA: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone_a' };
const stored = (pinnedIds: readonly string[], orderedIds: readonly string[] = []) =>
  JSON.stringify({ version: 2, pinnedIds, orderedIds });
const failure = (kind: HomeFailure['kind']) => (error: unknown) =>
  error instanceof HomeFailure && error.kind === kind;
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
  assert.deepEqual([...f.saved.values()], [stored(['it_two', 'it_one'])]);
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
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_one']));
  await f.home.pin('it_two');
  assert.equal(f.saved.get(homeOwnerKey(ownerB)!), stored(['it_two']));
});

test('returning to the same owner waits its old write queue before restoring durable favorites', async () => {
  const f = fixture();
  await f.home.activate();
  const pending = deferred<void>();
  const originalSet = f.port.set;
  f.port.set = async (key, value) => {
    await pending.promise;
    await originalSet(key, value);
  };
  const first = assert.rejects(
    f.home.pin('it_one'),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  const queued = assert.rejects(
    f.home.pin('it_two'),
    (error: unknown) => error instanceof HomeFailure && error.kind === 'accountChanged',
  );
  await Promise.resolve();
  f.owner(null);
  await f.home.refresh();
  f.owner(ownerA);
  const returning = f.home.refresh();
  assert.equal(f.home.getSnapshot().loading, true);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  // The accepted old write commits only to its original key. The old queued
  // callback is rejected; restoration then reads that settled durable result.
  pending.resolve();
  await Promise.all([first, queued, returning]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one']);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored(['it_one']));
  assert.equal(f.home.getSnapshot().error, null);
  f.home.dispose();
  await f.home.activate();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one']);
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

test('versioned local preferences reject malformed envelopes and retain bounded IDs only', () => {
  assert.deepEqual(parseHomePreference(stored(['it_one', 'it_one'], ['it_two', 'it_two'])), {
    pinnedIds: ['it_one'],
    orderedIds: ['it_two'],
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
    assert.deepEqual(parseHomePreference(JSON.stringify(value)), { pinnedIds: [], orderedIds: [] });
  assert.deepEqual(parseHomePreference(' '.repeat(32_769)), { pinnedIds: [], orderedIds: [] });
  const ids = Array.from({ length: 100 }, (_, index) => `${index}`.padEnd(128, 'a'));
  assert.deepEqual(parseHomePreference(stored(ids, ids)), { pinnedIds: ids, orderedIds: ids });
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
