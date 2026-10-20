import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

type Element = { type: unknown; props: Record<string, any> };
const compiled = await Promise.all(
  ['chat-header.ios.tsx', 'chat-title.tsx'].map(async (name) => {
    const source = new URL(`../components/${name}`, import.meta.url);
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
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react-native': {
      Pressable: 'Pressable',
      View: 'View',
      Text: 'Text',
      useWindowDimensions: () => ({ width, fontScale }),
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
      }),
    },
    './primitives': { Icon: 'Icon' },
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
  const header = load(compiled[0]!);
  const title = load(compiled[1]!);
  const props = {
    id: 'space-one',
    title: 'A real conversation with a long name',
    counterpart: { id: 'person', tint_hex: '#123' },
    activityLabel: 'typing',
    search: { active: false, onToggle: () => searches++ },
  };
  return {
    props,
    header: () => header.ChatHeader!(props),
    title: () => title.ChatTitle!(props),
    paths,
    searches: () => searches,
  };
}

test('native header keeps Back, actual Details and Search reachable with a separate avatar, without dead call controls', () => {
  const f = fixture();
  const all = nodes(f.header());
  assert.equal(
    all.find((node) => node.type === 'StackScreen')!.props.options.headerBackVisible,
    true,
  );
  const search = all.find((node) => node.type === 'ToolbarButton')!;
  assert.equal(search.props.icon, 'magnifyingglass');
  search.props.onPress();
  assert.equal(f.searches(), 1);
  const avatarButton = all.find((node) => node.props.testID === 'chat-details-avatar')!;
  assert.equal(avatarButton.props.style({ pressed: false }).width, 44);
  assert.equal(nodes(avatarButton).find((node) => node.type === 'ProfileAvatar')!.props.size, 38);
  avatarButton.props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(f.paths)), [
    { pathname: '/space/[id]', params: { id: 'space-one' } },
  ]);
  assert.equal(
    all.some((node) => ['phone', 'video'].includes(node.props.systemImage)),
    false,
  );
});

test('native title centers the source typography and keeps details plus long-press search, including narrow and larger text layouts', () => {
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
