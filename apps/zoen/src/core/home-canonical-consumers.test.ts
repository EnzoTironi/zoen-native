import assert from 'node:assert/strict';
import test from 'node:test';
import type { TimelineEntry } from './types';
import type { ConversationTimelineRow } from './conversation-timeline';
import {
  canonical,
  embedded,
  entries,
  fixture,
  renderScreen,
  nodes,
  type Element,
} from './home-canonical-consumers.fixtures.ts';

test('hidden Home shortcuts retain current canonical cards in actual Chat/Thread screens and management can pin them again', async (context) => {
  for (const screen of ['Chat', 'Thread'] as const)
    await context.test(screen, async () => {
      const f = await fixture();
      assert.equal(f.home().items.length, 0);
      assert.equal(f.home().allItems[0], canonical);
      const rendered = nodes(renderScreen(screen, f.home()));
      if (screen === 'Chat') {
        const rail = rendered.find((node) => node.type === 'MiniappRail');
        assert.ok(rail);
        assert.equal(
          rail.props.onRestore,
          undefined,
          'Hidden conversation Apps add no rail banner',
        );
        assert.deepEqual(rail.props.items, []);
      }
      const list = rendered.find((node) => node.type === 'FlashList');
      assert.ok(list, 'Actual conversation screen must render its timeline');
      const rows = list.props.data as ConversationTimelineRow[];
      const shared = rows.find((row) => row.entry.id === 'shared')!;
      const message = nodes(
        list.props.renderItem({ item: shared, index: rows.indexOf(shared) }),
      ).find((node) => node.type === 'ConversationMessage')!;
      assert.equal(
        message.props.item,
        canonical,
        'Hidden shortcut must still resolve the latest authorized App for message cards',
      );
      const activity = rows.find((row) => row.activity?.item?.id === canonical.id)?.activity;
      assert.ok(activity, 'Hidden shortcut must retain its canonical service activity');
      assert.equal(activity.item, canonical);
      assert.ok(!('showCard' in activity), 'Updates do not repeat an App cover');
      assert.equal(
        activity.entries.length,
        2,
        'Adjacent edits still coalesce while the Home shortcut is hidden',
      );
      assert.equal(f.controller.getSnapshot().items[0], canonical);
    });
  await context.test('management Pin restores the shortcut atomically', async () => {
    const f = await fixture();
    const header = nodes(renderScreen('MiniappScreen', f.home())).find(
      (node) => node.type === 'StackScreen',
    );
    assert.ok(header);
    const menu = header.props.options.headerRight() as Element;
    assert.equal(menu.type, 'HomeCardMenu');
    assert.equal(
      typeof menu.props.onToggle,
      'function',
      'Hidden authorized App must retain its management Pin action',
    );
    menu.props.onToggle();
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.deepEqual(f.controller.getSnapshot().pinnedIds, [canonical.id]);
    assert.deepEqual(f.controller.getSnapshot().hiddenIds, []);
    assert.equal(f.controller.getSnapshot().items[0], canonical);
  });
});

test('actual Chat and Thread pass every complete signed album to the shared renderer, while unsupported native versions keep individual rows', async () => {
  const f = await fixture();
  for (const screen of ['Chat', 'Thread'] as const) {
    const root = entries(screen === 'Thread')[0]!;
    const members: TimelineEntry[] = [0, 1].map((index) => ({
      ...root,
      id: `album-member-${index}`,
      seq: String(index + 2),
      kind: {
        Message: {
          text: `Canonical caption ${index}`,
          card: { ...embedded, kind_id: 'file', app: null, item_id: `file-${index}` },
        },
      },
      media: { presentation: 'media', group: { id: `album_${'a'.repeat(64)}`, index, count: 2 } },
    }));
    for (const supported of [false, true]) {
      const list = nodes(renderScreen(screen, f.home(), [root, ...members], supported)).find(
        (node) => node.type === 'FlashList',
      )!;
      const rows = list.props.data as ConversationTimelineRow[];
      assert.equal(rows.length, supported ? 2 : 3);
      assert.deepEqual(
        rows.flatMap((row) => row.album?.entries ?? [row.entry]),
        [root, ...members],
      );
      if (supported) {
        const row = rows[1]!;
        const rendered = nodes(list.props.renderItem({ item: row, index: 1 })).find(
          (node) => node.type === 'ConversationMessage',
        )!;
        assert.strictEqual(
          rendered.props.album,
          row.album,
          `${screen} must not hide projected children without passing their canonical album to the renderer`,
        );
      }
    }
  }
});

for (const screen of ['Chat'] as const)
  test(`${screen} hands initial-end ownership to an explicit quote interaction`, async () => {
    const f = await fixture();
    let cancelled = 0;
    const quote = nodes(
      renderScreen(screen, f.home(), undefined, false, () => {
        cancelled++;
      }),
    ).find((node) => node.type === 'QuoteLayoutTransition');
    assert.ok(quote);
    assert.equal(typeof quote.props.onBegin, 'function');
    quote.props.onBegin();
    assert.equal(cancelled, 1);
  });

test('Thread uses the same scoped quote motion and reading policy as the main conversation', async () => {
  const f = await fixture();
  const rendered = nodes(renderScreen('Thread', f.home()));
  assert.ok(rendered.find((node) => node.type === 'QuoteLayoutTransition'));
  const list = rendered.find((node) => node.type === 'FlashList');
  assert.ok(list);
  assert.equal(list.props.CellRendererComponent, 'QuoteMotionView');
  assert.equal(typeof list.props.onCommitLayoutEffect, 'function');
  assert.equal(typeof list.props.onScrollBeginDrag, 'function');
});
