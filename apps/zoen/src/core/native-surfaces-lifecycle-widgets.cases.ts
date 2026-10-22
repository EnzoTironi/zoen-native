import assert from 'node:assert/strict';
import test from 'node:test';
import { openSurfaceInvocation } from '../native-surfaces/intents.ts';
import {
  surfaceItemURL,
  surfaceLinkPath,
  surfaceRouteInvocation,
} from '../native-surfaces/links.ts';
import { privateWidgetTitle, widgetSelectionId } from '../native-surfaces/widget-catalog.ts';
import { item, owner, widgetFixture } from './native-surfaces-lifecycle.fixtures.ts';

test('independent widget selections update one privacy choice and follow current canonical versions', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, false);
  const initial = fixture.catalog();
  assert.deepEqual(
    initial.entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_a'), widgetSelectionId(owner, 'it_b')],
  );
  assert.equal(initial.entries[0]?.content.redacted, false);
  assert.equal(initial.entries[1]?.content.redacted, true);
  assert.equal(initial.entries[1]?.configurationTitle, privateWidgetTitle(2));
  assert.equal(JSON.stringify(initial.entries[1]).includes('Private event'), false);

  await fixture.coordinator.selectWidget('it_a', 3, false);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_a', showContent: false },
    { itemId: 'it_b', showContent: true },
  ]);
  const current = fixture.environment.items.get('it_b')!;
  fixture.environment.items.set('it_b', { ...current, version: 4 });
  await fixture.coordinator.refresh();
  const updated = fixture.catalog();
  assert.deepEqual(
    updated.entries.map((entry) => entry.selectionId),
    initial.entries.map((entry) => entry.selectionId),
  );
  assert.equal(updated.entries[0]?.content.redacted, true);
  assert.equal(updated.entries[1]?.content.redacted, false);
  assert.equal(updated.entries[1]?.content.deepLink, surfaceItemURL(owner, 'it_b', 4));
  await assert.rejects(fixture.coordinator.selectWidget('it_b', 3, false), /app has changed/);
  assert.equal(fixture.catalog().entries[1]?.content.redacted, false);
});

test('removing one widget preserves every other selection, key, privacy choice and shortcut', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, false);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  await fixture.coordinator.selectShortcut('it_b', 3, true);
  const second = fixture.catalog().entries[1];
  await fixture.coordinator.removeWidget('it_a');
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_b', showContent: true },
  ]);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.shortcutItems, ['it_b']);
  assert.deepEqual(fixture.catalog().entries, [second]);
  await fixture.coordinator.removeWidget('it_missing');
  assert.deepEqual(fixture.catalog().entries, [second]);
  await fixture.coordinator.removeWidget('it_b');
  assert.deepEqual(fixture.catalog().entries, []);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.shortcutItems, ['it_b']);
});

test('deleted and inaccessible selected items disappear independently without selecting a fallback', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  fixture.environment.items.set('it_unselected', item(Date.now() + 60_000, 'it_unselected'));
  fixture.environment.items.delete('it_a');
  await fixture.coordinator.refresh();
  assert.deepEqual(
    fixture.catalog().entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_b')],
  );
  fixture.environment.spaces.delete('sp_b');
  await fixture.coordinator.refresh();
  assert.deepEqual(fixture.catalog().entries, []);
  assert.deepEqual(
    fixture.coordinator.getSnapshot().preferences?.widgets.map((selection) => selection.itemId),
    ['it_a', 'it_b'],
  );
  fixture.environment.items.set('it_a', { ...item(), version: 5 });
  await fixture.coordinator.refresh();
  assert.deepEqual(
    fixture.catalog().entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_a')],
  );
  assert.equal(fixture.catalog().entries[0]?.content.deepLink, surfaceItemURL(owner, 'it_a', 5));
});

test('failed multi-widget saves and removals retain the last committed catalog and can retry', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  const first = fixture.catalog();
  const stored = fixture.stored();
  fixture.environment.failWrite = true;
  await assert.rejects(fixture.coordinator.selectWidget('it_b', 3, false), /storage unavailable/);
  assert.equal(fixture.catalog(), first);
  assert.equal(fixture.stored(), stored);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_a', showContent: true },
  ]);
  fixture.environment.failWrite = false;
  await fixture.coordinator.selectWidget('it_b', 3, false);
  const both = fixture.catalog();
  fixture.environment.failWrite = true;
  await assert.rejects(fixture.coordinator.removeWidget('it_a'), /storage unavailable/);
  assert.equal(fixture.catalog(), both);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 2);
  fixture.environment.failWrite = false;
  await fixture.coordinator.removeWidget('it_a');
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_b', showContent: false },
  ]);
});

test('twenty widgets allow exact updates and removal while a twenty-first add never writes preferences', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  for (let index = 0; index < 21; index++)
    fixture.environment.items.set(`it_${index}`, item(Date.now() + 60_000, `it_${index}`));
  for (let index = 0; index < 20; index++)
    await fixture.coordinator.selectWidget(`it_${index}`, 3, false);
  const writes = fixture.native.writes.length;
  await assert.rejects(fixture.coordinator.selectWidget('it_20', 3, true), /20/);
  assert.equal(fixture.native.writes.length, writes);
  assert.equal(fixture.catalog().entries.length, 20);
  await fixture.coordinator.selectWidget('it_5', 3, true);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 20);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets[5], {
    itemId: 'it_5',
    showContent: true,
  });
  await fixture.coordinator.removeWidget('it_3');
  await fixture.coordinator.selectWidget('it_20', 3, true);
  assert.deepEqual(
    fixture.coordinator.getSnapshot().preferences?.widgets.map((selection) => selection.itemId),
    [
      ...Array.from({ length: 20 }, (_, index) => `it_${index}`).filter((id) => id !== 'it_3'),
      'it_20',
    ],
  );
});

test('a late item read cannot publish an old-owner widget catalog and old native links cannot open the new account', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  const oldLink = fixture.catalog().entries[0]!.content.deepLink!;
  const oldKey = fixture.catalog().entries[0]!.selectionId;
  let entered!: () => void;
  const reading = new Promise<void>((resolve) => {
    entered = resolve;
  });
  let finish!: () => void;
  const waiting = new Promise<void>((resolve) => {
    finish = resolve;
  });
  fixture.environment.beforeBatch = async () => {
    entered();
    await waiting;
  };
  const publishedBefore = fixture.native.widgets.length;
  const refresh = fixture.coordinator.refresh();
  await reading;
  fixture.environment.owner = { identityId: 'id_b', deviceId: 'dev_b' };
  finish();
  await refresh;
  fixture.environment.beforeBatch = null;
  await fixture.coordinator.refresh();
  assert.ok(
    fixture.native.widgets.slice(publishedBefore).every((catalog) => !catalog?.entries.length),
  );
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, []);
  await fixture.coordinator.selectWidget('it_a', 3, true);
  assert.notEqual(fixture.catalog().entries[0]!.selectionId, oldKey);

  const path = surfaceLinkPath(oldLink);
  assert.ok(path);
  const opened: string[] = [];
  const result = await openSurfaceInvocation(
    surfaceRouteInvocation(Object.fromEntries(new URLSearchParams(path.split('?')[1]))),
    {
      currentOwner: () => fixture.environment.owner,
      item: async () => fixture.environment.items.get('it_a')!,
      canOpenSpace: async (id) => fixture.environment.spaces.has(id),
      open: (route) => {
        opened.push(route);
      },
    },
  );
  assert.deepEqual(result, { status: 'owner_changed' });
  assert.deepEqual(opened, []);
});

test('one bounded owner-scoped batch refreshes all selected widgets and shortcuts without full-item reads', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, false);
  await fixture.coordinator.selectShortcut('it_a', 3, true);
  fixture.native.batches.length = 0;
  fixture.native.singleReads.length = 0;
  await fixture.coordinator.refresh();
  assert.deepEqual(fixture.native.singleReads, []);
  assert.equal(fixture.native.batches.length, 1);
  assert.deepEqual(fixture.native.batches[0], { ids: ['it_a', 'it_b'], owner });
  assert.equal(fixture.catalog().entries.length, 2);
});

test('membership is rechecked after a delayed canonical batch and denied items never reach native publication', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  let entered!: () => void;
  const reading = new Promise<void>((resolve) => {
    entered = resolve;
  });
  let finish!: () => void;
  const waiting = new Promise<void>((resolve) => {
    finish = resolve;
  });
  fixture.environment.beforeBatch = async () => {
    entered();
    await waiting;
  };
  const refresh = fixture.coordinator.refresh();
  await reading;
  // Replace rather than mutate the map so a pre-await captured access map would
  // still include B and fail this production-boundary regression.
  fixture.environment.spaces = new Map([['sp_a', 'Private chat A']]);
  finish();
  await refresh;
  assert.deepEqual(
    fixture.catalog().entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_a')],
  );
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 2);
});

test('a failed canonical batch clears native projections without substituting individual or cached item reads', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, false);
  fixture.native.batches.length = 0;
  fixture.native.singleReads.length = 0;
  fixture.environment.failBatch = true;
  await fixture.coordinator.refresh();
  assert.equal(fixture.native.widgets.at(-1), null);
  assert.equal(fixture.native.batches.length, 1);
  assert.deepEqual(fixture.native.singleReads, []);
  assert.match(fixture.coordinator.getSnapshot().error ?? '', /surfaces could not be updated/);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 2);
  fixture.environment.failBatch = false;
  await fixture.coordinator.refresh();
  assert.equal(fixture.catalog().entries.length, 2);
  assert.equal(fixture.coordinator.getSnapshot().error, null);
});

test('a delayed preference acknowledgment cannot publish the old owner choice into the next account', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  let entered!: () => void;
  const writing = new Promise<void>((resolve) => {
    entered = resolve;
  });
  let finish!: () => void;
  const waiting = new Promise<void>((resolve) => {
    finish = resolve;
  });
  fixture.environment.beforeWrite = async () => {
    entered();
    await waiting;
  };
  const baseline = fixture.native.widgets.length;
  const changing = fixture.coordinator.selectWidget('it_b', 3, true);
  const refused = assert.rejects(changing, /account has changed/);
  await writing;
  fixture.environment.owner = { identityId: 'identity_next', deviceId: 'device_next' };
  finish();
  await refused;
  fixture.environment.beforeWrite = null;
  await fixture.coordinator.refresh();
  assert.equal(fixture.stored(), null);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, []);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.owner, fixture.environment.owner);
  assert.ok(fixture.native.widgets.slice(baseline).every((catalog) => !catalog?.entries.length));
});
