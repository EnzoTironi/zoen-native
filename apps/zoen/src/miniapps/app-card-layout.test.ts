import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import { t } from '../i18n/index.ts';
import type { ItemDetail } from '../core/generated/ItemDetail';
import type { ItemCard } from '../core/generated/ItemCard';
import { APP_CARD_METRICS as M } from './app-card-layout.ts';

type AppComponents = Record<
  | 'CompactMiniappCard'
  | 'MiniappMessageCard'
  | 'MiniappRail'
  | 'MiniappTimelineActivityView'
  | 'Home'
  | 'AppCardEditActions'
  | 'NativeHomeActions',
  (props: any) => Element
>;
type Element = { type: string | ((props: any) => unknown); props: Record<string, any> };
const sources = {
  react: ['useState', 'useRef', 'useCallback', 'useLayoutEffect'],
  'react-native': ['View', 'Text', 'Pressable', 'ActivityIndicator', 'useWindowDimensions'],
  'expo-router': ['router'],
  '@expo/ui': ['Host'],
  '@expo/ui/swift-ui': ['Button', 'Menu'],
  '@expo/ui/swift-ui/modifiers': ['buttonStyle', 'frame', 'labelStyle'],
  'expo-image': ['Image'],
  'react-native-reanimated': ['useAnimatedRef'],
  primitives: ['Icon', 'EmptyState', 'NativeButton'],
  interaction: ['MessagePressable'],
  theme: ['useTheme'],
  i18n: ['t', 'formatLocale', 'locale'],
  reorder: ['ReorderableAppCards', 'useCardScrollPort'],
  sky: ['SkyScene'],
  menus: ['HomeActions', 'HomeCardMenu'],
  home: ['useHome'],
  receipt: ['MessageReceipt', 'messageDeliveryLabel'],
  presentation: ['messageText'],
} satisfies Record<string, string[]>;
const aliases: Record<string, string> = {
  '../components/primitives': 'primitives',
  '../../components/primitives': 'primitives',
  '../components/message-interaction': 'interaction',
  '../theme': 'theme',
  '../../theme': 'theme',
  '../i18n/index.ts': 'i18n',
  '../i18n': 'i18n',
  '../../i18n/index.ts': 'i18n',
  './reorderable-cards': 'reorder',
  '../../miniapps/reorderable-cards': 'reorder',
  '../../components/sky-background': 'sky',
  '../../home/menus': 'menus',
  '../../home/provider': 'home',
  '../components/message-receipt': 'receipt',
  '../core/presentation': 'presentation',
};
const compiled = await build({
  stdin: {
    contents: `export { CompactMiniappCard } from './compact-card';
      export { MiniappMessageCard } from './message-card';
      export { MiniappRail } from './rail';
      export { MiniappTimelineActivityView } from './timeline-activity';
      export { default as Home } from '../app/(tabs)/index';
      export { AppCardEditActions } from './card-edit-actions';
      export { HomeActions as NativeHomeActions } from '../home/menus.ios';`,
    resolveDir: fileURLToPath(new URL('.', import.meta.url)),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'cover-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (path === 'react/jsx-runtime' || path === 'cover-test-ports')
            return { path, external: true };
          if (/\.(svg|jpg)$/.test(path)) return { path, namespace: 'cover-art' };
          const name = aliases[path] ?? path;
          if (name in sources) return { path: name, namespace: 'cover-port' };
        });
        builder.onLoad({ filter: /./, namespace: 'cover-art' }, ({ path }) => ({
          contents: `export default ${JSON.stringify(path)};`,
        }));
        builder.onLoad({ filter: /./, namespace: 'cover-port' }, ({ path }) => ({
          contents:
            `const p=require('cover-test-ports')[${JSON.stringify(path)}];\n` +
            (path === 'react-native-reanimated' ? 'export default p.default;\n' : '') +
            sources[path as keyof typeof sources]
              .map((name) => `export const ${name}=p.${name};`)
              .join('\n'),
        }));
      },
    },
  ],
});

function item(template: string, extra: Record<string, unknown> = {}): ItemDetail {
  const id = `app_${template.replaceAll('.', '_')}`;
  return {
    id,
    space_id: 'chat',
    kind_id: 'app',
    version: 1,
    title: `Canonical full title with many words and an ending that must remain accessible — ${template}`,
    app: {
      app_id: 'synthetic',
      headline: 'Full canonical summary remains available',
      snapshot_json: JSON.stringify({
        id,
        appId: 'synthetic',
        deepLink: `zoen://app/${id}`,
        template,
        title: 'A longer App title that deliberately spans more than one line',
        eyebrow: 'Saturday',
        value: '2/8',
        detail: 'Two portions ready',
        accentHex: '#376B53',
        symbol: 'square.grid.2x2',
        art: 'pot',
        ...extra,
      }),
    },
  } as ItemDetail;
}
function fixture(
  fontScale = 1,
  width = 393,
  options: {
    stateValues?: readonly unknown[];
    items?: ItemDetail[];
    hiddenItems?: ItemDetail[];
  } = {},
) {
  let stateIndex = 0;
  const routes: unknown[] = [];
  const homeItems = options.items ?? [
    item('stat'),
    item('countdown', { targetMs: 1_800_000_000_000 }),
  ];
  const ports = {
    react: {
      useRef: (value: unknown) => ({ current: value }),
      useCallback: (value: unknown) => value,
      useLayoutEffect: (effect: () => unknown) => effect(),
      useState: (value: unknown) => [
        options.stateValues && stateIndex < options.stateValues.length
          ? options.stateValues[stateIndex++]
          : typeof value === 'function'
            ? value()
            : value,
        () => {},
      ],
    },
    'react-native': {
      View: 'View',
      Text: 'Text',
      Pressable: 'Pressable',
      ActivityIndicator: 'ActivityIndicator',
      useWindowDimensions: () => ({ width, height: 852, fontScale }),
    },
    'expo-router': { router: { push: (route: unknown) => routes.push(route) } },
    'expo-image': { Image: 'Image' },
    '@expo/ui': { Host: 'NativeHost' },
    '@expo/ui/swift-ui': { Button: 'NativeButton', Menu: 'NativeMenu' },
    '@expo/ui/swift-ui/modifiers': {
      buttonStyle: () => ({}),
      frame: (value: unknown) => value,
      labelStyle: () => ({}),
    },
    'react-native-reanimated': {
      default: { ScrollView: 'ScrollView' },
      useAnimatedRef: () => ({}),
    },
    primitives: { Icon: 'Icon', EmptyState: 'EmptyState', NativeButton: 'NativeButton' },
    interaction: { MessagePressable: 'Pressable' },
    theme: {
      useTheme: () => ({
        reducedMotion: true,
        colors: { surface: '#fff', text: '#111', secondary: '#555', accent: '#396' },
        presentation: { chat: { service: { background: '#FFFFFFCC', text: '#555555' } } },
      }),
    },
    i18n: { t, formatLocale: 'en-US', locale: 'en' },
    reorder: {
      ReorderableAppCards: 'ReorderableAppCards',
      useCardScrollPort: () => ({ extent: { set() {} } }),
    },
    sky: { SkyScene: 'SkyScene' },
    menus: { HomeActions: 'HomeActions', HomeCardMenu: 'HomeCardMenu' },
    home: {
      useHome: () => ({
        items: homeItems,
        pinnedItems: [],
        hiddenItems: options.hiddenItems ?? [],
        ownerKey: 'owner',
        isPinned: () => false,
        pendingRequestCount: 0,
      }),
    },
    receipt: { MessageReceipt: 'MessageReceipt', messageDeliveryLabel: () => 'Sending' },
    presentation: { messageText: () => 'Updated app' },
  };
  const module = { exports: {} as AppComponents };
  const jsx = (type: Element['type'], props: Element['props']) => ({ type, props });
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    console,
    require: (name: string) =>
      name === 'cover-test-ports'
        ? ports
        : name === 'react/jsx-runtime'
          ? { jsx, jsxs: jsx, Fragment: 'Fragment' }
          : assert.fail(name),
  });
  return { ...module.exports, routes };
}
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const element = value as Element;
  if (typeof element.type === 'function') return nodes(element.type(element.props));
  return [element, ...nodes(element.props.children)];
}
const style = (node: Element) =>
  typeof node.props.style === 'function' ? node.props.style({ pressed: false }) : node.props.style;
const cardFor = (tree: Element, id: string) =>
  nodes(tree).find((n) => n.props.testID === `miniapp-card-${id}`)!;

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
  const done = nodes(home.Home({})).find((node) => node.props.testID === 'home-rearrange-done')!;
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
