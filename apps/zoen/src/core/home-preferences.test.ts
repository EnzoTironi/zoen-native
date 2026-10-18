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

test('pins cannot race ahead of a pending restore and overwrite an existing selection', async () => {
  const f = fixture();
  const pins = deferred<string | null>();
  f.port.get = () => pins.promise;
  const loading = f.home.activate();
  await Promise.resolve();
  await assert.rejects(
    f.home.pin('it_one'),
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
