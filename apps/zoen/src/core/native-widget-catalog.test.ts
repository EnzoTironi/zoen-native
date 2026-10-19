import assert from 'node:assert/strict';
import test from 'node:test';
import { emptyPreferences, parsePreferences } from '../native-surfaces/preferences.ts';
import type { WidgetSelection } from '../native-surfaces/preferences';
import { surfaceItemURL } from '../native-surfaces/links.ts';
import type { SnapshotItem, SurfaceOwner } from '../native-surfaces/snapshot';
import {
  MAX_WIDGET_SELECTIONS,
  privateWidgetTitle,
  validWidgetSelectionId,
  widgetCatalog,
  widgetSelectionId,
} from '../native-surfaces/widget-catalog.ts';

const owner = { identityId: 'identity_a', deviceId: 'device_a' };
const spaces = new Set(['space_a', 'space_b']);

function item(id: string, title: string, spaceId = 'space_a', version = 3): SnapshotItem {
  const app = {
    app_id: 'countdown',
    resource_uri: 'zoen://apps/countdown',
    name: 'Countdown',
    view_json: '{"secret":"private app state"}',
    headline: 'Private headline',
    metrics: [],
    last_action: 'Private command result',
    trust: 'Suggest',
    snapshot_json: JSON.stringify({
      id,
      appId: 'countdown',
      template: 'countdown',
      title,
      detail: `Private details for ${title}`,
      targetMs: 1_900_000_000_000,
      accentHex: '#123456',
      symbol: 'timer',
      deepLink: `zoen://app/${id}`,
      actions: [{ tool: 'countdown_change', label: 'Change' }],
    }),
  };
  return {
    id,
    space_id: spaceId,
    version,
    app,
  };
}

test('configuration keys are opaque and stable per account/device/item, independent of current content or version', () => {
  const a = widgetSelectionId(owner, 'item_a');
  assert.equal(a, widgetSelectionId({ ...owner }, 'item_a'));
  assert.equal(validWidgetSelectionId(a), true);
  assert.match(a, /^w_[a-f0-9]{64}$/);
  assert.equal(a.includes(owner.identityId), false);
  assert.equal(a.includes('item_a'), false);
  const alternatives = [
    widgetSelectionId(owner, 'item_b'),
    widgetSelectionId({ ...owner, identityId: 'identity_b' }, 'item_a'),
    widgetSelectionId({ ...owner, deviceId: 'device_b' }, 'item_a'),
  ];
  assert.equal(new Set([a, ...alternatives]).size, 4);
  assert.notEqual(
    widgetSelectionId({ identityId: 'a:b', deviceId: 'c' }, 'item_a'),
    widgetSelectionId({ identityId: 'a', deviceId: 'b:c' }, 'item_a'),
  );

  const selection = [{ itemId: 'item_a', showContent: true }];
  const first = widgetCatalog([item('item_a', 'Dinner')], owner, selection, spaces)!;
  const updated = widgetCatalog(
    [item('item_a', 'New dinner', 'space_a', 9)],
    owner,
    selection,
    spaces,
  )!;
  assert.equal(first.entries[0]?.selectionId, a);
  assert.equal(updated.entries[0]?.selectionId, a);
  assert.equal(updated.entries[0]?.configurationTitle, 'New dinner');
  assert.equal(updated.entries[0]?.content.deepLink, surfaceItemURL(owner, 'item_a', 9));
});

test('two configured widgets carry independent current content and explicit privacy choices', () => {
  const canonical = [item('item_a', 'Private dinner'), item('item_b', 'Private flight', 'space_b')];
  const catalog = widgetCatalog(
    canonical,
    owner,
    [
      { itemId: 'item_a', showContent: true },
      { itemId: 'item_b', showContent: false },
    ],
    spaces,
  )!;
  assert.equal(catalog.entries.length, 2);
  assert.equal(catalog.entries[0]?.configurationTitle, 'Private dinner');
  assert.equal(catalog.entries[0]?.content.title, 'Private dinner');
  assert.equal(catalog.entries[0]?.content.targetMs, 1_900_000_000_000);
  assert.equal(catalog.entries[1]?.configurationTitle, privateWidgetTitle(2));
  assert.equal(catalog.entries[1]?.content.redacted, true);
  assert.equal(catalog.entries[1]?.content.targetMs, undefined);
  assert.deepEqual(catalog.entries[1]?.content.bars, []);
  assert.equal(JSON.stringify(catalog.entries[1]).includes('Private flight'), false);
  assert.equal(JSON.stringify(catalog).includes('private app state'), false);
  assert.equal(JSON.stringify(catalog).includes('Private headline'), false);
  assert.equal(JSON.stringify(catalog).includes('countdown_change'), false);
  assert.equal(catalog.placeholder.redacted, true);
  assert.equal(catalog.placeholder.deepLink, undefined);
  assert.equal(JSON.stringify(catalog.placeholder).includes('Private dinner'), false);
});

test('missing, inaccessible and invalid items retain independent keys without a first-entry fallback', () => {
  const selections = [
    { itemId: 'item_a', showContent: true },
    { itemId: 'item_b', showContent: false },
  ];
  const b = item('item_b', 'Private flight', 'space_b');
  const other = item('item_unselected', 'Never selected');
  const deleted = widgetCatalog([b, other], owner, selections, spaces)!;
  assert.deepEqual(
    deleted.entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'item_b')],
  );
  assert.equal(deleted.entries[0]?.configurationTitle, privateWidgetTitle(2));
  const invalidA = item('item_a', 'Invalid original');
  invalidA.app!.snapshot_json = '{bad JSON';
  assert.deepEqual(widgetCatalog([invalidA, b, other], owner, selections, spaces), deleted);
  const inaccessible = widgetCatalog([b, other], owner, selections, new Set(['space_a']))!;
  assert.deepEqual(inaccessible.entries, []);
  assert.equal(inaccessible.placeholder.deepLink, undefined);
  assert.deepEqual(widgetCatalog([other], owner, [], spaces)?.entries, []);
  const foreign = widgetCatalog([b], { ...owner, deviceId: 'new_device' }, selections, spaces)!;
  assert.notEqual(foreign.entries[0]?.selectionId, deleted.entries[0]?.selectionId);
});

test('duplicate selection input cannot escalate reveal permission and retains preference-order private names', () => {
  const catalog = widgetCatalog(
    [item('item_a', 'Private A'), item('item_b', 'Private B')],
    owner,
    [
      { itemId: 'item_a', showContent: false },
      { itemId: 'item_a', showContent: true },
      { itemId: 'item_b', showContent: false },
    ],
    spaces,
  )!;
  assert.deepEqual(
    catalog.entries.map((entry) => entry.configurationTitle),
    [privateWidgetTitle(1), privateWidgetTitle(3)],
  );
  assert.equal(catalog.entries[0]?.content.redacted, true);
  assert.equal(JSON.stringify(catalog).includes('Private A'), false);
  assert.equal(JSON.stringify(catalog).includes('Private B'), false);
});

test('catalog boundaries reject malformed scope/choices and preserve the twenty-selection limit', () => {
  assert.equal(MAX_WIDGET_SELECTIONS, 20);
  const all = Array.from({ length: 20 }, (_, index) => item(`item_${index}`, `Item ${index}`));
  const selections = all.map((entry) => ({ itemId: entry.id, showContent: false }));
  assert.equal(widgetCatalog(all, owner, selections, spaces)?.entries.length, 20);
  assert.equal(
    widgetCatalog(all, owner, [...selections, { itemId: 'item_20', showContent: false }], spaces),
    null,
  );
  for (const malformed of [
    null,
    {},
    [{ itemId: '../outside', showContent: true }],
    [{ itemId: 'item_a', showContent: 'yes' }],
  ]) {
    assert.equal(
      widgetCatalog(all, owner, malformed as unknown as WidgetSelection[], spaces),
      null,
    );
  }
  for (const invalid of [{ identityId: '../other', deviceId: 'device_a' }, null]) {
    assert.equal(widgetCatalog(all, invalid as unknown as SurfaceOwner, selections, spaces), null);
    assert.throws(() => widgetSelectionId(invalid as unknown as SurfaceOwner, 'item_a'));
  }
  for (const invalid of [
    'item_a',
    'w_short',
    `w_${'a'.repeat(63)}`,
    `w_${'G'.repeat(64)}`,
    null,
    7,
  ]) {
    assert.equal(validWidgetSelectionId(invalid), false);
  }
  assert.throws(() => widgetSelectionId(owner, '../outside'));
  for (const invalid of [0, 21, 1.5, NaN]) assert.throws(() => privateWidgetTitle(invalid));
});

test('unsupported schema-one storage restores no widget, shortcut or activity opt-in', () => {
  const activity = {
    id: 'activity_a',
    linkId: 'link_a',
    itemId: 'item_b',
    endsAt: 1_900_000_000_000,
    showContent: false,
  };
  for (const showContent of [false, true]) {
    const legacy = {
      schemaVersion: 1,
      owner,
      widget: { itemId: 'item_a', showContent, content: 'Never restore cached content' },
      shortcutItems: ['item_b', 'item_b'],
      activity,
      privateState: 'Never copy this field',
    };
    const current = parsePreferences(JSON.stringify(legacy), owner);
    assert.deepEqual(current, emptyPreferences(owner));
    assert.equal('widget' in current, false);
    assert.equal(JSON.stringify(current).includes('Never'), false);
  }
});

test('schema-two parsing owns copied choices, keeps first privacy decision and rejects foreign or invalid storage', () => {
  const source = {
    ...emptyPreferences(owner),
    widgets: [
      { itemId: 'item_a', showContent: false },
      { itemId: 'item_a', showContent: true },
      { itemId: 'item_b', showContent: true },
    ],
  };
  const parsed = parsePreferences(JSON.stringify(source), owner);
  source.widgets[0]!.showContent = true;
  assert.deepEqual(parsed.widgets, [
    { itemId: 'item_a', showContent: false },
    { itemId: 'item_b', showContent: true },
  ]);
  assert.deepEqual(
    parsePreferences(JSON.stringify(source), { ...owner, deviceId: 'other' }).widgets,
    [],
  );
  const malformed = [
    { ...source, schemaVersion: 3 },
    { ...source, widgets: null },
    { ...source, widgets: [{ itemId: 'item_a' }] },
    { ...source, widgets: [{ itemId: 'item_a', showContent: 1 }] },
    {
      ...source,
      activity: {
        id: 'activity_a',
        itemId: 'item_a',
        endsAt: 1_900_000_000_000,
        showContent: true,
      },
    },
    {
      ...source,
      widgets: Array.from({ length: 21 }, (_, index) => ({
        itemId: `item_${index}`,
        showContent: false,
      })),
    },
  ];
  for (const value of malformed)
    assert.deepEqual(parsePreferences(JSON.stringify(value), owner), emptyPreferences(owner));
  for (const json of ['{bad', 'null', '[]', 'x'.repeat(8_193)])
    assert.deepEqual(parsePreferences(json, owner), emptyPreferences(owner));
});
