import assert from 'node:assert/strict';
import { test } from 'node:test';
import { homeOwnerKey, projectHomeItems } from '../home/preferences.ts';
import { fixture, miniapp, ownerA, ownerB, stored } from './home-preferences.fixtures.ts';

test('draft previews every shortcut field; Discard restores baseline without storage writes', async () => {
  const f = fixture();
  await f.home.activate();
  await f.home.pin('it_one');
  const before = f.saved.get(homeOwnerKey(ownerA)!);
  let writes = 0;
  f.port.set = async () => {
    writes++;
  };
  f.home.beginEdit();
  await f.home.reorderItems(['it_two', 'it_one'], ownerA, ['it_one', 'it_two']);
  await f.home.pin('it_two');
  await f.home.hide('it_one');
  assert.equal(f.home.getSnapshot().editing, true);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  assert.deepEqual(
    projectHomeItems(f.home.getSnapshot().items, f.home.getSnapshot()).items.map((i) => i.id),
    ['it_two'],
  );
  assert.equal(writes, 0);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), before);
  f.home.discardEdit();
  assert.equal(f.home.getSnapshot().editing, false);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_one']);
  assert.deepEqual(f.home.getSnapshot().orderedIds, []);
  assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
  assert.equal(writes, 0);
});

test('Done persists the full preview once in the existing key without copying canonical content', async () => {
  const f = fixture();
  await f.home.activate();
  const canonical = f.home.getSnapshot().items;
  const writes: string[] = [];
  f.port.set = async (_key, value) => {
    writes.push(value);
  };
  f.home.beginEdit();
  await f.home.reorderItems(['it_two', 'it_one'], ownerA, ['it_one', 'it_two']);
  await f.home.pin('it_two');
  await f.home.hide('it_one');
  await f.home.commitEdit();
  assert.deepEqual(writes, [stored(['it_two'], ['it_two', 'it_one'], ['it_one'])]);
  assert.equal(f.home.getSnapshot().items, canonical);
  assert.equal(f.home.getSnapshot().editing, false);
  assert.equal(f.home.getSnapshot().editSaving, false);
  await assert.rejects(f.home.commitEdit());
  assert.equal(writes.length, 1);
});

test('retired queued preview cannot fall through to an immediate write; old owner cannot commit or edit', async () => {
  const f = fixture();
  await f.home.activate();
  let writes = 0;
  f.port.set = async () => {
    writes++;
  };
  f.home.beginEdit();
  const queued = f.home.hide('it_one');
  f.home.discardEdit();
  await assert.rejects(queued);
  assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
  assert.equal(writes, 0);
  f.home.beginEdit();
  await f.home.hide('it_one');
  f.owner(ownerB);
  await f.home.refresh();
  assert.equal(f.home.getSnapshot().editing, false);
  assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
  await assert.rejects(f.home.commitEdit(ownerA));
  assert.throws(() => f.home.requireEdit(ownerA));
  assert.equal(writes, 0);
});

test('membership changes retire the draft; failed Done retains explicit recovery without automatic retry', async () => {
  const f = fixture();
  await f.home.activate();
  f.home.beginEdit();
  await f.home.hide('it_one');
  f.items([miniapp('it_two')]);
  await f.home.refresh();
  assert.equal(f.home.getSnapshot().editing, false);
  assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
  f.home.beginEdit();
  await f.home.pin('it_two');
  let writes = 0;
  f.port.set = async () => {
    writes++;
    throw new Error('synthetic write failure');
  };
  await assert.rejects(f.home.commitEdit());
  assert.equal(writes, 1);
  assert.equal(f.home.getSnapshot().editing, true);
  assert.equal(f.home.getSnapshot().editSaving, false);
  assert.deepEqual(f.home.getSnapshot().pinnedIds, ['it_two']);
  f.home.discardEdit();
  assert.deepEqual(f.home.getSnapshot().pinnedIds, []);
  assert.equal(writes, 1);
});

test('unchanged Done preserves legacy bytes; a same-owner session revision retires and fences a draft', async () => {
  const f = fixture();
  let revision = 1;
  Object.assign(f.source, { revision: () => revision });
  f.saved.set(homeOwnerKey(ownerA)!, '["it_one"]');
  await f.home.activate();
  f.home.beginEdit();
  await f.home.commitEdit();
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_one"]');
  f.home.beginEdit();
  await f.home.hide('it_one');
  revision++;
  await assert.rejects(f.home.commitEdit());
  assert.throws(() => f.home.requireEdit());
  await f.home.refresh();
  assert.equal(f.home.getSnapshot().editing, false);
  assert.deepEqual(f.home.getSnapshot().hiddenIds, []);
  assert.equal(f.saved.get(homeOwnerKey(ownerA)!), '["it_one"]');
});
