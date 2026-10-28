import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';
import { navigationGlassAppearance } from '../components/navigation-appearance.ts';

type Element = { type: unknown; props: Record<string, any> };
const compiled = await Promise.all(
  [
    '../components/chat-header.ios.tsx',
    '../components/chat-title.tsx',
    './telegram-chat-header-layout.ts',
    '../components/navigation-glass.tsx',
    '../components/chats-actions.ios.tsx',
    '../calls/header-action.ios.tsx',
    '../components/message-selection-toolbar.tsx',
  ].map(async (name) => {
    const source = new URL(name, import.meta.url);
    const output = await build({
      stdin: { contents: readFileSync(source, 'utf8'), loader: 'tsx' },
      bundle: true,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
      plugins: [
        {
          name: 'native-chrome-ports',
          setup(builder) {
            builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
          },
        },
      ],
    });
    return output.outputFiles[0]!.text;
  }),
);
function nodes(tree: unknown): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (!tree || typeof tree !== 'object' || !('props' in tree)) return [];
  const node = tree as Element;
  return [node, ...nodes(node.props.children)];
}
function fixture(
  platform: 'ios' | 'android' = 'ios',
  width = 402,
  fontScale = 1,
  glass = true,
  calling = false,
) {
  const paths: unknown[] = [];
  const callRequests: unknown[] = [];
  const calls = {
    available: calling,
    start: async (space: string, video: boolean) => {
      callRequests.push({ space, video });
    },
  };
  const chat = {
    id: 'space-one',
    kind: 'Direct',
    privacy: 'EndToEnd',
    members: [{ id: 'me' }, { id: 'person' }],
  };

  let searches = 0;
  let backs = 0;
  let measurement: unknown = null;
  const currentRender = { current: '' };
  const frame = { width, fontScale };
  const safe = { top: 62, left: 0, right: 0, bottom: 34 };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    react: {
      useRef: () => currentRender,
      useLayoutEffect: (effect: () => void) => effect(),
      useState: () => [
        measurement,
        (next: any) => {
          measurement = typeof next === 'function' ? next(measurement) : next;
        },
      ],
    },
    'react-native-safe-area-context': { useSafeAreaInsets: () => safe },
    'react-native': {
      Pressable: 'Pressable',
      View: 'View',
      Text: 'Text',
      useWindowDimensions: () => frame,
      PixelRatio: { get: () => 3 },
    },
    'expo-router': {
      router: { push: (path: unknown) => paths.push(path) },
      Stack: {
        Screen: 'StackScreen',
        Title: 'StackTitle',
        Toolbar: Object.assign('StackToolbar', { Button: 'ToolbarButton', View: 'ToolbarView' }),
      },
    },
    'expo-glass-effect': { GlassView: 'GlassView', isLiquidGlassAvailable: () => glass },
    '@expo/ui': { Host: 'Host', Button: 'Button' },
    '@expo/ui/swift-ui': { Button: 'Button', HStack: 'HStack', Menu: 'Menu' },
    '@expo/ui/swift-ui/modifiers': Object.fromEntries(
      ['buttonStyle', 'font', 'foregroundStyle', 'frame', 'labelStyle', 'tint'].map((name) => [
        name,
        (value: unknown) => ({ [name]: value }),
      ]),
    ),
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en' },
    '../calls/provider': { useNativeCalls: () => calls },
    './provider': { useNativeCalls: () => calls },
    '../core/provider': { useClient: () => ({ snapshot: { conversations: [chat] } }) },
    '../components/dialogs': { useDialogs: () => ({ inform: async () => {} }) },
    '../profile/profile-avatar': { ProfileAvatar: 'ProfileAvatar' },
    '../saved-messages/conversation-avatar': { ConversationAvatar: 'ConversationAvatar' },
    '../theme': {
      useTheme: () => ({
        colors: { text: '#fff', secondary: '#aaa', accent: '#9f0' },
        scheme: 'dark',
        presentation: telegramPresentation('night'),
      }),
    },
    './primitives': { Icon: 'Icon', IconButton: 'IconButton' },
    './navigation-container': { NavigationContainer: 'View' },
    './chat-title': { ChatTitle: 'ChatTitle' },
    './navigation-appearance': { navigationGlassAppearance },
  };
  const load = (code: string) => {
    const module = { exports: {} as Record<string, (props: any) => Element> };
    runInNewContext(code, {
      module,
      exports: module.exports,
      process: { env: { EXPO_OS: platform } },
      require: (path: string) => {
        assert.ok(path in ports, path);
        return ports[path];
      },
    });
    return module.exports;
  };
  const geometry = load(compiled[2]!);
  ports['../core/telegram-chat-header-layout'] = geometry;
  const material = load(compiled[3]!);
  ports['./navigation-glass'] = material;
  const action = load(compiled[5]!);
  ports['../calls/header-action.ios'] = action;
  ports['./message-selection-toolbar'] = load(compiled[6]!);
  const header = load(compiled[0]!);
  const title = load(compiled[1]!);
  const chats = load(compiled[4]!);
  const props = {
    id: 'space-one',
    scrollViewTag: 1234,
    title: 'A real conversation with a long name',
    saved: false,
    counterpart: { id: 'person', kind: 'Person', tint_hex: '#123' },
    activityLabel: 'typing',
    search: { active: false, onToggle: () => searches++ },
  };
  return {
    props,
    calls,
    chat,
    callRequests,
    action: () => action.CallHeaderAction!({ space: props.id }),
    header: () => header.ChatHeader!(props),
    title: () => title.ChatTitle!(props),
    inbox: () => chats.ChatsActions!({}),
    material: material.NavigationGlass!,
    bar() {
      const element =
        header.ChatHeader!(props).props.options.unstable_headerRightItems()[0].element;
      return element.type(element.props) as Element;
    },
    titleWithBounds(availableWidth: number, onLayout?: (event: any) => void) {
      return title.ChatTitle!({ ...props, availableWidth, onLayout });
    },
    geometry,
    frame,
    safe,
    paths,
    backs: () => backs,
    searches: () => searches,
  };
}

test('native bar keeps the inbox backdrop and Back while title, Details and Search remain actionable', () => {
  const f = fixture();
  const options = f.header().props.options;
  assert.equal(options.headerShown, true);
  assert.equal(options.header, undefined);
  assert.equal(options.headerLeft, undefined);
  assert.equal(options.headerBackButtonDisplayMode, 'minimal');
  assert.equal(options.headerTransparent, true);
  assert.equal(options.headerStyle.backgroundColor, 'transparent');
  assert.equal(options.headerLargeStyle.backgroundColor, 'transparent');
  assert.equal(options.headerBlurEffect, 'none');
  assert.equal(options.headerBackground, undefined);
  assert.equal(options.headerRight, undefined);
  assert.equal(options.unstable_headerRightItems()[0].hidesSharedBackground, true);
  assert.equal(options.scrollEdgeEffects.top, 'automatic');
  assert.equal(options.scrollEdgeEffects.bottom, 'hidden');
  assert.equal(options.gestureEnabled, undefined);
  const container = options.headerTitle();
  assert.equal(container.props.scrollViewTag, 1234);
  const title = container.props.children;
  assert.equal(title.type, 'ChatTitle');
  assert.equal(title.props.activityLabel, 'typing');
  assert.equal(title.props.id, 'space-one');
  const all = nodes(f.bar());
  assert.equal(f.bar().type, f.material);
  const search = all.find((node) => node.props.testID === 'chat-search')!;
  assert.equal(search.props.accessibilityLabel, 'chat.searchThisChat');
  search.props.onPress();
  assert.equal(f.searches(), 1);
  const avatar = all.find((node) => node.props.testID === 'chat-details-avatar')!;
  assert.equal(avatar.props.style({ pressed: false }).width, 44);
  assert.equal(nodes(avatar).find((node) => node.type === 'ProfileAvatar')!.props.size, 38);
  avatar.props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(f.paths)), [
    { pathname: '/space/[id]', params: { id: 'space-one' } },
  ]);
  f.props.search.active = true;
  assert.equal(
    nodes(f.bar()).find((node) => node.props.testID === 'chat-search')!.props.accessibilityLabel,
    'chat.closeSearch',
  );
});

test('inbox, conversation actions and title share one native material without a second bar background', () => {
  for (const available of [true, false]) {
    const f = fixture('ios', 402, 1, available);
    const host = nodes(f.inbox()).find((node) => node.type === 'Host')!;
    assert.equal(host.props.ignoreSafeArea, 'all');
    assert.equal(host.props.style.width, 96);
    assert.equal(host.props.style.height, 44);
    const surfaces = [f.inbox(), f.bar(), ...nodes(f.title()).filter((n) => n.type === f.material)];
    assert.equal(surfaces.length, 3);
    for (const surface of surfaces) {
      assert.equal(surface.type, f.material);
      const rendered = f.material(surface.props);
      assert.equal(rendered.type, available ? 'GlassView' : 'View');
      if (available) {
        assert.equal(rendered.props.glassEffectStyle, 'regular');
        assert.equal(rendered.props.isInteractive, true);
        assert.equal(rendered.props.colorScheme, 'dark');
        assert.equal(rendered.props.tintColor, undefined);
      } else {
        assert.equal(
          rendered.props.style.backgroundColor,
          telegramPresentation('night').navigation.background,
        );
      }
      assert.equal(rendered.props.style.opacity, undefined);
      assert.equal(rendered.props.style.overflow, undefined);
    }
  }
});

test('title centers its own source typography and preserves details/long-press without claiming native window placement', () => {
  for (const [width, scale] of [
    [402, 1],
    [320, 1.8],
  ]) {
    const f = fixture('ios', width, scale);
    const tree = f.title();
    const all = nodes(tree);
    const texts = all.filter((node) => node.type === 'Text');
    assert.equal(texts[0]!.props.style.fontSize, 17);
    assert.equal(texts[0]!.props.style.textAlign, 'center');
    assert.equal(texts[0]!.props.numberOfLines, 1);
    assert.equal(
      all.some((node) => node.type === 'ProfileAvatar'),
      false,
    );
    const content = all.find((node) => node.props.testID === 'chat-title-content')!;
    assert.equal(content.props.style.minHeight, 44);
    assert.ok(content.props.style.minWidth <= content.props.style.maxWidth);
    assert.ok(content.props.style.maxWidth < width);
    assert.equal(texts.length, scale > 1.2 ? 1 : 2);
    if (scale === 1) assert.equal(texts[1]!.props.style.fontSize, 12);
    tree.props.onPress();
    tree.props.onLongPress();
    assert.equal(f.paths.length, 1);
    assert.equal(f.searches(), 1);
  }
  const other = fixture('android');
  assert.ok(nodes(other.title()).some((node) => node.type === 'ProfileAvatar'));
  assert.equal(other.title().props.onLongPress, undefined);
});

test('title bound reaches the actual Pressable and can be narrower than the former88pt floor', () => {
  const f = fixture();
  let layouts = 0;
  const title = f.titleWithBounds(76, () => layouts++);
  title.props.onLayout({});
  assert.equal(layouts, 1);
  const content = nodes(title).find((node) => node.props.testID === 'chat-title-content')!;
  assert.equal(content.props.style.minWidth, 76);
  assert.equal(content.props.style.maxWidth, 76);
  title.props.onPress();
  title.props.onLongPress();
  assert.equal(f.paths.length, 1);
  assert.equal(f.searches(), 1);
});

test('source-derived containing geometry covers portrait, both notch edges and missing Back without guessed UIKit widths', () => {
  const f = fixture();
  const layout = f.geometry.chatHeaderLayout as any;
  const place = f.geometry.chatHeaderTitlePlacement as any;
  for (const [width, top, left, right] of [
    [402, 62, 0, 0],
    [320, 20, 0, 0],
    [874, 0, 62, 0],
    [874, 0, 0, 62],
  ]) {
    const bounds = layout({ width, top, left, right, back: true, search: true });
    assert.equal(bounds.height, top + 60);
    assert.equal(bounds.controlTop, top + 10);
    assert.equal(bounds.controlTop + 44, bounds.height - 6);
    assert.equal(bounds.leading, left + 16);
    assert.equal(bounds.trailing + 88, width - right - 16);
    for (const natural of [80, 178, 190 + 2 / 3, 1000]) {
      const title = place(width, bounds, natural, 3);
      assert.ok(title.x >= bounds.leftTitleInset - 1e-8);
      assert.ok(title.x + title.width <= width - bounds.rightTitleInset + 1e-8);
      if (natural <= width - 2 * Math.max(bounds.leftTitleInset, bounds.rightTitleInset)) {
        assert.ok(Math.abs(title.x + title.width / 2 - width / 2) <= 1 / 3);
      }
    }
  }
  const noSearch = layout({ width: 402, top: 62, left: 0, right: 0, back: true, search: false });
  assert.equal(place(402, noSearch, 178, 3).x, 112);
  const noBack = layout({ width: 402, top: 62, left: 0, right: 0, back: false, search: true });
  const centered = place(402, noBack, 178, 3);
  assert.equal(centered.x, 114);
  assert.equal(centered.width, 174);
  assert.equal(centered.x + centered.width / 2, 201);
});

test('voice/video share one native call menu and unavailable or non-private conversations expose no call control', async () => {
  const f = fixture('ios', 402, 1, true, true);
  const bar = nodes(f.bar());
  const call = bar.find(
    (node) => typeof node.type === 'function' && node.props.space === f.props.id,
  );
  assert.ok(call);
  const menu = nodes(f.action()).find((node) => node.type === 'Menu')!;
  menu.props.onPrimaryAction();
  const video = nodes(menu).find((node) => node.props.systemImage === 'video')!;
  video.props.onPress();
  await Promise.resolve();
  assert.deepEqual(JSON.parse(JSON.stringify(f.callRequests)), [
    { space: 'space-one', video: false },
    { space: 'space-one', video: true },
  ]);
  f.chat.privacy = 'Public';
  assert.equal(
    nodes(f.bar()).some((node) => node.props.space === f.props.id),
    false,
  );
  f.chat.privacy = 'EndToEnd';
  f.calls.available = false;
  assert.equal(
    nodes(f.bar()).some((node) => node.props.space === f.props.id),
    false,
  );
});

test('Saved Messages opens its media and never exposes calls or group details', () => {
  const f = fixture('ios', 402, 1, true, true);
  f.chat.kind = 'saved';
  f.props.saved = true;
  const bar = nodes(f.bar());
  assert.equal(
    bar.some((node) => node.props.space === f.props.id),
    false,
  );
  assert.ok(bar.some((node) => node.type === 'ConversationAvatar'));
  bar.find((node) => node.props.testID === 'chat-details-avatar')!.props.onPress();
  f.title().props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(f.paths)), [
    { pathname: '/shared-media/[id]', params: { id: 'space-one' } },
    { pathname: '/shared-media/[id]', params: { id: 'space-one' } },
  ]);
});

test('selection owns native count and Cancel, then restores the original conversation controls', () => {
  const f = fixture();
  let cancelled = 0;
  const props = f.props as typeof f.props & { selection?: { count: number; onCancel: () => void } };
  props.selection = { count: 3, onCancel: () => cancelled++ };
  const selected = f.header().props.options;
  assert.equal(selected.headerBackVisible, false);
  assert.equal(selected.unstable_headerLeftItems().length, 0);
  const title = selected.headerTitle();
  const rendered = title.type(title.props);
  assert.equal(rendered.props.children, 3);
  assert.equal(rendered.props.accessibilityLabel, 'chat.selectedMessages');
  rendered.props.onAccessibilityAction({ nativeEvent: { actionName: 'cancel' } });
  const items = selected.unstable_headerRightItems();
  assert.equal(items.length, 1);
  assert.equal(items[0].type, 'button');
  assert.equal(items[0].label, 'chat.cancel');
  items[0].onPress();
  assert.equal(cancelled, 2);
  delete props.selection;
  const restored = f.header().props.options;
  assert.equal(restored.headerBackVisible, true);
  assert.equal(restored.unstable_headerLeftItems, undefined);
  assert.equal(restored.unstable_headerRightItems()[0].type, 'custom');
  assert.equal(restored.title, props.title);
});
