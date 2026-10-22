import assert from 'node:assert/strict';
import test from 'node:test';
import type { ItemCard } from '../core/generated/ItemCard';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t } from '../i18n/index.ts';
import { cardFor, fixture, item, nodes, style } from './app-card-layout.fixtures.ts';
import { APP_CARD_METRICS as M } from './app-card-layout.ts';

test('actual mixed native templates keep 160 pt frames, full labels and bounded scaled slots without font shrinking', () => {
  const mixed = [
    item('stat'),
    item('progress', {
      art: 'pet',
      bars: [
        { label: 'Food', value: 0.8 },
        { label: 'Mood', value: 0.4 },
      ],
    }),
    item('countdown', { value: undefined, targetMs: 1_800_000_000_000 }),
    item('list', { rows: [{ text: 'Bring water', done: true }] }),
    item('caption'),
    item('ticket', {
      codes: ['GRU', 'SFO'],
      places: ['São Paulo', 'San Francisco'],
      times: ['08:30', '11:45'],
    }),
    item('photo', { photo: 'hike-tomales', value: undefined }),
    {
      ...item('stat'),
      id: 'external',
      app: {
        ...item('stat').app!,
        app_id: 'external',
        headline: 'External app summary',
        snapshot_json: '',
      },
    } as ItemDetail,
  ];
  for (const fontScale of [1, 1.2, 1.5, 1.8, 2.2, 3.2])
    for (const editActions of [undefined, { type: 'EditActions', props: {} }]) {
      const f = fixture(fontScale);
      for (const value of mixed) {
        const tree = f.CompactMiniappCard!({ item: value, editActions });
        assert.equal(style(tree).width, 160);
        assert.equal(style(tree).height, 160);
        const card = cardFor(tree, value.id);
        assert.equal(style(card).width, 160);
        assert.equal(style(card).height, 160);
        assert.equal(style(card).borderRadius, 24);
        assert.equal(style(card).minHeight, undefined);
        assert.ok(card.props.accessibilityLabel.includes(value.title));
        assert.ok(card.props.accessibilityLabel.includes(value.app!.headline));
        const all = nodes(tree);
        for (const text of all.filter((n) => n.type === 'Text')) {
          assert.equal(text.props.adjustsFontSizeToFit, undefined);
          assert.equal(text.props.minimumFontScale, undefined);
          assert.equal(text.props.maxFontSizeMultiplier, undefined);
          assert.notEqual(text.props.allowFontScaling, false);
          assert.ok(text.props.numberOfLines > 0);
        }
        const slots = all.filter((n) =>
          /miniapp-card-(visual|title|metadata)-/.test(n.props.testID ?? ''),
        );
        for (const slot of slots) {
          const box = style(slot);
          assert.ok(box.top >= 12);
          assert.ok(box.top + box.height <= 148, `${fontScale} ${slot.props.testID}`);
          for (const text of nodes(slot).filter((n) => n.type === 'Text'))
            assert.ok(
              style(text).lineHeight * fontScale * text.props.numberOfLines <= box.height + 0.01,
            );
        }
        const title = slots.find((n) => n.props.testID.includes('-title-'))!;
        assert.ok(title);
        assert.ok(nodes(title).some((n) => n.type === 'Text' && n.props.children));
        const visual = slots.find((n) => n.props.testID.includes('-visual-'));
        const metadata = slots.find((n) => n.props.testID.includes('-metadata-'));
        if (visual) assert.ok(style(visual).top + style(visual).height < style(title).top);
        if (metadata) assert.ok(style(title).top + style(title).height < style(metadata).top);
        if (fontScale > 1.2) assert.equal(metadata, undefined);
        if (fontScale >= 2.2) assert.equal(visual, undefined);
        if (editActions) {
          const menu = all.find((n) => n.props.children === editActions)!;
          assert.equal(style(menu).left, 12);
          assert.equal(style(menu).right, 12);
          assert.equal(style(menu).height, 44);
          assert.equal(visual, undefined);
          assert.ok(style(title).top >= 56);
        }
      }
    }
});

test('normal pet, ticket and photo covers show real state and keep artwork and metadata within their slots', () => {
  const f = fixture();
  const pet = item('progress', { art: 'pet', bars: [{ label: 'Food', value: 0.8 }] });
  const petNodes = nodes(f.CompactMiniappCard!({ item: pet }));
  assert.ok(petNodes.some((n) => n.type === 'Text' && n.props.children === 'Food'));
  assert.ok(petNodes.some((n) => style(n)?.width === '80%'));
  const ticket = item('ticket', { codes: ['GRU', 'SFO'], times: ['08:30', '11:45'] });
  const ticketTree = f.CompactMiniappCard!({ item: ticket });
  for (const label of ['GRU', 'SFO', '08:30', '11:45']) {
    assert.ok(nodes(ticketTree).some((n) => n.type === 'Text' && n.props.children === label));
    assert.ok(cardFor(ticketTree, ticket.id).props.accessibilityLabel.includes(label));
  }
  for (const scale of [1, 1.2]) {
    const tree = fixture(scale).CompactMiniappCard!({ item: ticket });
    const visual = nodes(tree).find((n) => n.props.testID === `miniapp-card-visual-${ticket.id}`)!;
    const text = nodes(visual).filter((n) => n.type === 'Text');
    const codes = text.filter((n) => ['GRU', 'SFO'].includes(n.props.children));
    const times = text.filter((n) => ['08:30', '11:45'].includes(n.props.children));
    assert.equal(codes.length, 2);
    assert.equal(times.length, 2);
    assert.ok(
      Math.max(...codes.map((n) => style(n).lineHeight * scale)) +
        3 +
        Math.max(...times.map((n) => style(n).lineHeight * scale)) <=
        style(visual).height,
    );
  }
  const photo = item('photo', { photo: 'hike-tomales' });
  const photoTree = f.CompactMiniappCard!({ item: photo });
  assert.ok(nodes(photoTree).some((n) => n.type === 'Image' && n.props.contentFit === 'cover'));
  assert.ok(nodes(photoTree).some((n) => style(n)?.experimental_backgroundImage?.includes('0.68')));
  assert.ok(
    nodes(photoTree)
      .filter((n) => n.type === 'Text')
      .every((n) => ['#FFFFFF', '#E1EAF0'].includes(style(n).color)),
  );
});

test('inline messages and rails share the canonical App tile; activity remains a service event', () => {
  const f = fixture(3.2);
  const value = item('stat');
  const card = {
    item_id: value.id,
    title: value.title,
    kind_id: 'app',
    app: value.app,
    version: 1,
  } as ItemCard;
  for (const width of [136, 240, 350])
    for (const spaceId of ['chat', undefined]) {
      const tree = f.MiniappMessageCard!({ card, width, spaceId });
      assert.equal(style(tree).width, 160);
      assert.equal(style(tree).height, 160);
      assert.equal(style(cardFor(tree, value.id)).height, 160);
      assert.ok(
        !nodes(tree).some(
          (n) => n.type === 'Text' && n.props.children === t('miniapps.openMiniapp'),
        ),
      );
    }
  const rail = f.MiniappRail!({ items: [value] });
  assert.equal(style(cardFor(rail, value.id)).width, 160);
  const editableRail = f.MiniappRail!({ items: [value], onReorder: async () => {} });
  const reorder = nodes(editableRail).find((n) => n.type === 'ReorderableAppCards')!;
  assert.equal(reorder.props.size, undefined);
  assert.equal(reorder.props.gap, undefined);
  const activity = f.MiniappTimelineActivityView!({
    activity: {
      item: value,
      entries: [
        {
          id: 'change',
          at_ms: '1000',
          author: { name: 'Ana' },
          kind: { ItemEdited: { item_id: value.id, title: value.title, version: 1 } },
        },
      ],
    },
  });
  assert.equal(
    cardFor(activity, value.id),
    undefined,
    'Updates are service events, not another cover',
  );
  const generic = f.MiniappMessageCard!({
    card: { ...card, kind_id: 'note', app: null, summary: 'Shared page' },
    width: 280,
  });
  assert.equal(style(generic).width, 280);
  assert.equal(style(generic).height, undefined);
  assert.ok(
    nodes(generic).some(
      (n) => n.type === 'Text' && n.props.children === t('miniapps.openSharedItem'),
    ),
  );
});

test('actual Home grid changes column count without changing tile dimensions at narrow, phone and tablet widths', () => {
  for (const [width, columns] of [
    [320, 1],
    [393, 2],
    [720, 4],
    [1024, 4],
  ])
    for (const scale of [1, 3.2]) {
      const f = fixture(scale, width);
      const tree = f.Home!({});
      const grid = nodes(tree).find((n) => n.type === 'ReorderableAppCards')!;
      assert.equal(grid.props.columns, columns);
      assert.equal(grid.props.size, undefined);
      assert.equal(grid.props.gap, undefined);
      assert.ok(columns * M.size + (columns - 1) * M.gap <= Math.min(width, 720) - 40);
    }
});

test('zero visible Apps retain the real restore action without a negative-width reorder surface', () => {
  const f = fixture();
  let restored = 0;
  const tree = f.MiniappRail({
    items: [],
    onReorder: async () => {},
    onRestore: () => {
      restored++;
    },
    restoreLabel: 'Restore hidden apps (2)',
  });
  const all = nodes(tree);
  const restore = all.find((node) => node.props.testID === 'app-rail-restore')!;
  assert.ok(restore);
  assert.equal(restore.props.label, 'Restore hidden apps (2)');
  restore.props.onPress();
  assert.equal(restored, 1);
  assert.ok(all.every((node) => node.type !== 'ReorderableAppCards' && node.type !== 'ScrollView'));
});
