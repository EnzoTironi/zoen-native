import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';

type Element = { type: unknown; props: Record<string, any> };
const compiled = await Promise.all(
  [
    '../components/chat-header.ios.tsx',
    '../components/chat-title.tsx',
    './telegram-chat-header-layout.ts',
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
function fixture(platform: 'ios' | 'android' = 'ios', width = 402, fontScale = 1) {
  const paths: unknown[] = [];
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
    'expo-glass-effect': { GlassView: 'GlassView', isLiquidGlassAvailable: () => true },
    '../i18n/index.ts': { t: (key: string) => key },
    '../profile/profile-avatar': { ProfileAvatar: 'ProfileAvatar' },
    '../theme': {
      useTheme: () => ({
        colors: { text: '#fff', secondary: '#aaa', accent: '#9f0' },
        scheme: 'dark',
        presentation: telegramPresentation('night'),
      }),
    },
    './primitives': { Icon: 'Icon' },
    './navigation-container': { NavigationContainer: 'View' },
    './chat-title': { ChatTitle: 'ChatTitle' },
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
  const header = load(compiled[0]!);
  const title = load(compiled[1]!);
  const props = {
    id: 'space-one',
    scrollViewTag: 1234,
    title: 'A real conversation with a long name',
    counterpart: { id: 'person', tint_hex: '#123' },
    activityLabel: 'typing',
    search: { active: false, onToggle: () => searches++ },
  };
  return {
    props,
    header: () => header.ChatHeader!(props),
    title: () => title.ChatTitle!(props),
    bar(back = true) {
      const screen = header.ChatHeader!(props);
      const element = screen.props.options.header({
        back: back ? { title: 'Chats' } : undefined,
        navigation: { goBack: () => backs++ },
      });
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

test('public custom header is the only bar and preserves real Back, title, Details and Search', () => {
  const f = fixture();
  const screen = f.header();
  const options = screen.props.options;
  assert.equal(options.headerShown, true);
  assert.equal(options.headerTransparent, true);
  assert.equal(typeof options.header, 'function');
  assert.equal(options.headerTitle, undefined);
  assert.equal(options.headerLeft, undefined);
  assert.equal(options.gestureEnabled, undefined);
  assert.equal(nodes(screen).filter((node) => node.type === 'StackScreen').length, 1);
  assert.equal(
    nodes(screen).some((node) => node.type === 'StackTitle' || node.type === 'StackToolbar'),
    false,
  );
  const all = nodes(f.bar());
  assert.equal(f.bar().props.scrollViewTag, 1234);
  assert.equal(options.scrollEdgeEffects.top, 'automatic');
  assert.equal(options.scrollEdgeEffects.bottom, 'hidden');
  const back = all.find((node) => node.props.testID === 'chat-back')!;
  assert.equal(back.props.accessibilityLabel, 'access.back');
  back.props.onPress();
  assert.equal(f.backs(), 1);
  const search = all.find((node) => node.props.testID === 'chat-search')!;
  assert.equal(search.props.accessibilityLabel, 'chat.searchThisChat');
  search.props.onPress();
  assert.equal(f.searches(), 1);
  const title = all.find((node) => node.type === 'ChatTitle')!;
  assert.equal(title.props.activityLabel, 'typing');
  assert.equal(title.props.id, 'space-one');
  assert.equal(title.props.availableWidth, 218);
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
  assert.equal(
    nodes(f.bar(false)).some((node) => node.props.testID === 'chat-back'),
    false,
  );
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

test('rendered preferred title width centers first then clamps clear of the actual right group', () => {
  const f = fixture();
  let all = nodes(f.bar());
  assert.equal(
    all.find((node) => node.props.testID === 'chat-navigation-bar')!.props.style.height,
    122,
  );
  const title = all.find((node) => node.type === 'ChatTitle')!;
  title.props.onLayout({ nativeEvent: { layout: { width: 178 } } });
  all = nodes(f.bar());
  assert.equal(
    all.find((node) => node.props.testID === 'chat-title-position')!.props.style.left,
    110,
  );
  title.props.onLayout({ nativeEvent: { layout: { width: 190 + 2 / 3 } } });
  all = nodes(f.bar());
  assert.equal(
    all.find((node) => node.props.testID === 'chat-title-position')!.props.style.left,
    288 - (190 + 2 / 3),
  );
  const position = all.find((node) => node.props.testID === 'chat-title-position')!.props.style;
  assert.equal(position.top, 72);
  assert.equal(position.transform, undefined);
});

test('old conversation/viewport measurements cannot move the current title', () => {
  const f = fixture();
  const old = nodes(f.bar()).find((node) => node.type === 'ChatTitle')!;
  f.props.id = 'space-two';
  f.props.title = 'Another conversation';
  let current = nodes(f.bar()).find((node) => node.type === 'ChatTitle')!;
  current.props.onLayout({ nativeEvent: { layout: { width: 178 } } });
  old.props.onLayout({ nativeEvent: { layout: { width: 50 } } });
  assert.equal(
    nodes(f.bar()).find((node) => node.props.testID === 'chat-title-position')!.props.style.left,
    110,
  );
  f.frame.width = 874;
  f.safe.top = 0;
  f.safe.left = 62;
  const wide = nodes(f.bar());
  const fresh = wide.find((node) => node.type === 'ChatTitle')!;
  fresh.props.onLayout({ nativeEvent: { layout: { width: 178 } } });
  current.props.onLayout({ nativeEvent: { layout: { width: 100 } } });
  const after = nodes(f.bar());
  assert.equal(f.bar().props.scrollViewTag, 1234);
  assert.equal(
    after.find((node) => node.props.testID === 'chat-title-position')!.props.style.left,
    348,
  );
  assert.equal(
    after.find((node) => node.props.testID === 'chat-navigation-bar')!.props.style.height,
    60,
  );
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
