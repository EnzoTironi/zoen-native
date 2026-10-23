import assert from 'node:assert/strict';
import { test } from 'node:test';
import { homeOwnerKey, parseHomePreference } from '../home/preferences.ts';
import {
  deferred,
  failure,
  fixture,
  miniapp,
  ownerA,
  projection,
  stored,
} from './home-preferences.fixtures.ts';

test('legacy arrays and v2 envelopes stay byte-identical until an actual v3 preference mutation', async () => {
  for (const legacy of [
    '["it_two"]',
    JSON.stringify({ version: 2, pinnedIds: ['it_two'], orderedIds: ['it_two', 'it_one'] }),
  ]) {
    const f = fixture();
    const key = homeOwnerKey(ownerA)!;
    f.saved.set(key, legacy);
    let writes = 0;
    const originalSet = f.port.set;
    f.port.set = async (key, value) => {
      ++writes;
      await originalSet(key, value);
    };
    await f.home.activate();
    assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
    await f.home.pin('it_two');
    await f.home.restore('it_one');
    await f.home.restoreAll();
    await f.home.reorderItems(projection(f.home).visible);
    assert.equal(writes, 0);
    assert.equal(f.saved.get(key), legacy);
    await f.home.hide('it_one');
    assert.equal(writes, 1);
    assert.deepEqual(JSON.parse(f.saved.get(key)!), {
      version: 3,
      pinnedIds: ['it_two'],
      orderedIds: legacy.startsWith('[') ? [] : ['it_two', 'it_one'],
      hiddenIds: ['it_one'],
    });
  }
});

test('v3 parsing bounds every ID list and envelope, rejects unknown fields and makes hidden win over favorite', () => {
  assert.deepEqual(
    parseHomePreference(stored(['it_one', 'it_two'], ['it_two', 'it_one'], ['it_one', 'it_one'])),
    {
      pinnedIds: ['it_two'],
      orderedIds: ['it_two', 'it_one'],
      hiddenIds: ['it_one'],
    },
  );
  const empty = { pinnedIds: [], orderedIds: [], hiddenIds: [] };
  for (const malformed of [
    { version: 3, pinnedIds: [], orderedIds: [] },
    { version: 3, pinnedIds: [], orderedIds: [], hiddenIds: [], content: 'not a preference' },
    { version: 3, pinnedIds: [], orderedIds: [], hiddenIds: 'it_one' },
    { version: 3, pinnedIds: [], orderedIds: [], hiddenIds: ['../private'] },
    { version: 3, pinnedIds: [], orderedIds: [], hiddenIds: ['x'.repeat(129)] },
    { version: 3, pinnedIds: [], orderedIds: [], hiddenIds: Array(101).fill('it_one') },
    { version: 3, pinnedIds: [], orderedIds: [], hiddenIds: [{ id: 'it_one' }] },
    { version: 2, pinnedIds: [], orderedIds: [], hiddenIds: [] },
    { version: 4, pinnedIds: [], orderedIds: [], hiddenIds: [] },
  ])
    assert.deepEqual(parseHomePreference(JSON.stringify(malformed)), empty);
  assert.deepEqual(parseHomePreference(' '.repeat(49_153)), empty);
  const pins = Array.from({ length: 100 }, (_, index) => `p${index}`.padEnd(128, 'a'));
  const hidden = Array.from({ length: 100 }, (_, index) => `h${index}`.padEnd(128, 'a'));
  const maximum = stored(pins, pins, hidden);
  assert.ok(maximum.length > 32_768 && maximum.length < 49_152);
  assert.deepEqual(parseHomePreference(maximum), {
    pinnedIds: pins,
    orderedIds: pins,
    hiddenIds: hidden,
  });
});

test('hiding removes only the local shortcut and favorite, preserves canonical App content and survives reactivation', async () => {
  const f = fixture();
  await f.home.activate();
  await f.home.pin('it_one');
  const canonical = f.home.getSnapshot().items;
  await f.home.hide('it_one');
  assert.equal(f.home.getSnapshot().items, canonical);
  assert.equal(canonical[0]?.app?.snapshot_json, '{"private":"canonical only"}');
  assert.deepEqual(projection(f.home), {
    visible: ['it_two'],
    hidden: ['it_one'],
    all: ['it_one', 'it_two'],
  });
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), stored([], [], ['it_one']));
  f.home.dispose();
  await f.home.activate();
  assert.deepEqual(projection(f.home).hidden, ['it_one']);
  await f.home.restore('it_one');
  assert.deepEqual(projection(f.home).visible, ['it_one', 'it_two']);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
});

test('pinning a hidden App unhides and favorites in one acknowledged write without an intermediate publication', async () => {
  const f = fixture();
  f.saved.set(homeOwnerKey(ownerA)!, stored([], ['it_two', 'it_one'], ['it_one']));
  await f.home.activate();
  const pending = deferred<void>();
  const originalSet = f.port.set;
  const writes: string[] = [];
  f.port.set = async (key, value) => {
    writes.push(value);
    await pending.promise;
    await originalSet(key, value);
  };
  const published: [readonly string[], readonly string[]][] = [];
  f.home.subscribe(() =>
    published.push([f.home.getSnapshot().pinnedIds, f.home.getSnapshot().hiddenIds]),
  );
  const pin = f.home.pin('it_one');
  await Promise.resolve();
  assert.deepEqual(writes, [stored(['it_one'], ['it_two', 'it_one'])]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.deepEqual(projection(f.home).visible, ['it_two']);
  assert.equal(published.length, 0);
  pending.resolve();
  await pin;
  assert.deepEqual(published, [[['it_one'], []]]);
  assert.deepEqual(projection(f.home).visible, ['it_two', 'it_one']);
});

test('restore-all restores only currently authorized shortcuts and preserves unavailable saved IDs', async () => {
  const f = fixture();
  f.saved.set(
    homeOwnerKey(ownerA)!,
    stored(
      [],
      ['not_loaded', 'it_two', 'revoked', 'it_one'],
      ['it_one', 'revoked', 'not_loaded', 'it_two'],
    ),
  );
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('revoked', 'removed')]);
  await f.home.activate();
  assert.deepEqual(projection(f.home).hidden, ['it_two', 'it_one']);
  await f.home.restoreAll();
  assert.deepEqual(f.home.getSnapshot().hiddenIds, ['revoked', 'not_loaded']);
  assert.deepEqual(projection(f.home), {
    visible: ['it_two', 'it_one'],
    hidden: [],
    all: ['it_two', 'it_one'],
  });
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
});

test('visible reordering preserves hidden and unavailable positions, including default unsaved hidden positions', async () => {
  for (const order of [[], ['unavailable', 'it_one', 'it_two', 'it_three']]) {
    const f = fixture();
    f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
    f.saved.set(homeOwnerKey(ownerA)!, stored([], order, ['it_two']));
    await f.home.activate();
    const canonical = f.home.getSnapshot().items;
    await f.home.reorderItems(['it_three', 'it_one'], ownerA, ['it_one', 'it_three']);
    assert.deepEqual(f.home.getSnapshot().orderedIds, [
      ...(order.length ? ['unavailable'] : []),
      'it_three',
      'it_two',
      'it_one',
    ]);
    assert.deepEqual(projection(f.home), {
      visible: ['it_three', 'it_one'],
      hidden: ['it_two'],
      all: ['it_three', 'it_two', 'it_one'],
    });
    assert.equal(f.home.getSnapshot().items, canonical);
    await f.home.restore('it_two');
    assert.deepEqual(projection(f.home).visible, ['it_three', 'it_two', 'it_one']);
  }
});

test('queued hide invalidates a stale drag at serialized admission without dropping or reviving shortcuts', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  await f.home.activate();
  const pending = deferred<void>();
  const originalSet = f.port.set;
  const writes: string[] = [];
  f.port.set = async (key, value) => {
    writes.push(value);
    if (writes.length === 1) await pending.promise;
    await originalSet(key, value);
  };
  const hide = f.home.hide('it_two');
  const drag = assert.rejects(
    f.home.reorderItems(['it_three', 'it_two', 'it_one'], ownerA, ['it_one', 'it_two', 'it_three']),
    failure('save'),
  );
  await Promise.resolve();
  assert.equal(writes.length, 1);
  pending.resolve();
  await Promise.all([hide, drag]);
  assert.equal(writes.length, 1);
  assert.deepEqual(projection(f.home).visible, ['it_one', 'it_three']);
  await f.home.reorderItems(['it_three', 'it_one'], ownerA, ['it_one', 'it_three']);
  assert.deepEqual(projection(f.home).all, ['it_three', 'it_two', 'it_one']);
});

test('hide followed by restore still invalidates an older queued drag even when its visible ID set returns', async () => {
  const f = fixture();
  await f.home.activate();
  const hide = f.home.hide('it_two');
  const restore = f.home.restore('it_two');
  const drag = assert.rejects(
    f.home.reorderItems(['it_two', 'it_one'], ownerA, ['it_one', 'it_two']),
    failure('save'),
  );
  await Promise.all([hide, restore, drag]);
  assert.deepEqual(projection(f.home).visible, ['it_one', 'it_two']);
  assert.deepEqual(f.home.getSnapshot().orderedIds, []);
});

test('a queued pin that restores an App invalidates a drag based on the hidden layout', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  f.saved.set(homeOwnerKey(ownerA)!, stored([], [], ['it_two']));
  await f.home.activate();
  const pin = f.home.pin('it_two');
  const drag = assert.rejects(
    f.home.reorderItems(['it_three', 'it_one'], ownerA, ['it_one', 'it_three']),
    failure('save'),
  );
  await Promise.all([pin, drag]);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  assert.deepEqual(projection(f.home).visible, ['it_one', 'it_two', 'it_three']);
});

test('queued hide, restore, pin and unpin merge their latest confirmed fields without losing IDs', async () => {
  const f = fixture();
  await f.home.activate();
  const pending = deferred<void>();
  const originalSet = f.port.set;
  const writes: string[] = [];
  f.port.set = async (key, value) => {
    writes.push(value);
    if (writes.length === 1) await pending.promise;
    await originalSet(key, value);
  };
  const operations = [
    f.home.hide('it_one'),
    f.home.hide('it_two'),
    f.home.restore('it_one'),
    f.home.pin('it_two'),
    f.home.unpin('it_two'),
  ];
  await Promise.resolve();
  pending.resolve();
  await Promise.all(operations);
  assert.deepEqual(writes, [
    stored([], [], ['it_one']),
    stored([], [], ['it_one', 'it_two']),
    stored([], [], ['it_two']),
    stored(['it_two']),
    stored([]),
  ]);
  assert.deepEqual(projection(f.home).visible, ['it_one', 'it_two']);
});
