import assert from 'node:assert/strict';
import test from 'node:test';
import type { ItemCard } from '../core/generated/ItemCard';
import { t } from '../i18n/index.ts';
import type { Element } from './app-card-layout.fixtures.ts';
import { cardFor, fixture, item, nodes, style } from './app-card-layout.fixtures.ts';

test('selected native controls keep two distinct44pt hit targets, complete labels and real callbacks', () => {
  const f = fixture(3.2);
  const called: string[] = [];
  const tree = f.AppCardEditActions({
    id: 'app',
    name: 'Full app name',
    pinned: true,
    disabled: false,
    onRemove: () => called.push('remove'),
    onTogglePin: () => called.push('pin'),
  });
  const buttons = nodes(tree).filter((node) => node.type === 'Pressable');
  assert.equal(buttons.length, 2);
  for (const button of buttons) {
    assert.equal(style(button).width, 44);
    assert.equal(style(button).height, 44);
    assert.equal(button.props.accessibilityRole, 'button');
    assert.ok(button.props.accessibilityLabel.includes('Full app name'));
    button.props.onPress();
  }
  assert.deepEqual(called, ['remove', 'pin']);
  assert.ok(buttons[0]!.props.accessibilityLabel.startsWith('Remove shortcut'));
  assert.equal(buttons[1]!.props.accessibilityState.selected, true);
  const disabled = nodes(
    f.AppCardEditActions({
      id: 'app',
      name: 'App',
      pinned: false,
      disabled: true,
      onRemove() {},
      onTogglePin() {},
    }),
  ).filter((node) => node.type === 'Pressable');
  assert.ok(
    disabled.every((button) => button.props.disabled && button.props.accessibilityState.disabled),
  );
});

test('native Home Add menu exposes restore only for real hidden Apps and does not navigate during render', () => {
  const f = fixture();
  let restores = 0;
  const plain = nodes(f.NativeHomeActions({ hiddenCount: 0, onRestoreHidden: () => restores++ }));
  assert.ok(plain.every((node) => node.props.testID !== 'home-restore-hidden-apps'));
  const hidden = nodes(f.NativeHomeActions({ hiddenCount: 2, onRestoreHidden: () => restores++ }));
  const restore = hidden.find((node) => node.props.testID === 'home-restore-hidden-apps')!;
  assert.equal(restore.props.label, 'Restore hidden apps (2)');
  assert.equal(restores, 0);
  restore.props.onPress();
  assert.equal(restores, 1);
});

test('the same canonical App has identical cover content and geometry in the top rail and inline message; updates do not duplicate it', () => {
  const signature = (tree: Element, id: string) =>
    nodes(cardFor(tree, id)).map((node) => ({
      type: node.type,
      testID: node.props.testID,
      style: style(node),
      text: node.type === 'Text' ? node.props.children : undefined,
      source: node.type === 'Image' ? node.props.source : undefined,
      contentFit: node.props.contentFit,
      label: node.props.accessibilityLabel,
    }));
  for (const scale of [1, 3.2]) {
    const f = fixture(scale);
    for (const value of [
      item('stat'),
      item('progress', { art: 'pet', bars: [{ label: 'Food', value: 0.8 }] }),
      item('ticket', { codes: ['GRU', 'SFO'], times: ['08:30', '11:45'] }),
      item('photo', { photo: 'hike-tomales' }),
    ]) {
      const card = {
        item_id: value.id,
        kind_id: 'app',
        title: 'Older embedded title',
        version: 0,
        app: value.app,
      } as ItemCard;
      const rail = f.MiniappRail({ items: [value] });
      const inline = f.MiniappMessageCard({
        card,
        item: value,
        spaceId: value.space_id,
        width: 350,
      });
      const activity = f.MiniappTimelineActivityView({
        activity: {
          item: value,
          entries: [
            {
              id: 'change',
              at_ms: '1000',
              author: { name: 'Ana' },
              kind: {
                ItemEdited: { item_id: value.id, title: value.title, version: value.version },
              },
            },
          ],
        },
        mine: false,
        width: 350,
      });
      assert.deepEqual(signature(inline, value.id), signature(rail, value.id));
      assert.equal(cardFor(activity, value.id), undefined);
      assert.ok(nodes(inline).every((node) => node.props.children !== t('miniapps.openMiniapp')));
    }
  }
});

test('removing the final card cannot strand Done in Saving and restore stays reachable on empty Home', () => {
  const empty = fixture(1, 393, { stateValues: [true, true] });
  const rail = nodes(empty.MiniappRail({ items: [], onReorder: async () => {}, onRestore() {} }));
  const railDone = rail.find((node) => node.props.testID === 'app-rail-rearrange-done')!;
  assert.ok(railDone);
  assert.equal(railDone.props.disabled, false);
  assert.equal(railDone.props.label, t('home.doneRearranging'));
  const home = fixture(1, 393, {
    items: [],
    hiddenItems: [item('stat')],
    stateValues: ['owner', { pinned: true, available: true }],
  });
  const done = nodes(home.Home({})).find((node) => node.props.testID === 'app-card-edit-done')!;
  assert.ok(done);
  assert.equal(done.props.disabled, false);
  assert.equal(done.props.label, t('home.doneRearranging'));
  const settled = fixture(1, 393, { items: [], hiddenItems: [item('stat')] });
  const menu = nodes(settled.Home({})).find((node) => node.type === 'HomeActions')!;
  assert.equal(menu.props.hiddenCount, 1);
  assert.equal(typeof menu.props.onRestoreHidden, 'function');
});

test('entering rail editing keeps the held card before Done without inserting a header above it', () => {
  for (const label of [undefined, 'Apps']) {
    const before = fixture().MiniappRail({
      items: [item('stat')],
      label,
      onReorder: async () => {},
    });
    const after = fixture(1, 393, { stateValues: [true, false] }).MiniappRail({
      items: [item('stat')],
      label,
      onReorder: async () => {},
    });
    const children = (tree: Element) =>
      (tree.props.children as (Element | null)[]).filter(Boolean) as Element[];
    const normal = children(before),
      editing = children(after);
    const normalRail = normal.findIndex((node) => node.type === 'ScrollView');
    const editingRail = editing.findIndex((node) => node.type === 'ScrollView');
    assert.equal(editingRail, normalRail);
    assert.equal(normal[normalRail]!.props.scrollEnabled, true);
    assert.equal(editing[editingRail]!.props.scrollEnabled, false);
    assert.equal(
      JSON.stringify(editing.slice(0, editingRail)),
      JSON.stringify(normal.slice(0, normalRail)),
    );
    const doneRow = editing.findIndex((node) =>
      nodes(node).some((child) => child.props.testID === 'app-rail-rearrange-done'),
    );
    assert.ok(doneRow > editingRail);
    assert.equal(
      nodes(after).filter((node) => node.props.children === t('home.rearrange')).length,
      0,
    );
  }
});

test('header-managed rail keeps160pt cards with jiggle clearance and renders no bottom completion row', () => {
  const f = fixture();
  const tree = f.MiniappRail({
    items: [item('stat')],
    onReorder: async () => {},
    editing: true,
    onEditingChange() {},
    paddingVertical: 2,
  });
  const all = nodes(tree);
  assert.ok(all.every((n) => n.props.testID !== 'app-rail-rearrange-done'));
  const scroll = all.find((n) => n.type === 'ScrollView')!;
  assert.equal(scroll.props.scrollEnabled, false);
  assert.equal(scroll.props.contentContainerStyle.paddingVertical, 4);
  assert.equal(style(tree).paddingVertical, 2);
  assert.equal(all.find((n) => n.type === 'ReorderableAppCards')!.props.editing, true);
});
