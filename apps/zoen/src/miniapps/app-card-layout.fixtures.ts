import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t } from '../i18n/index.ts';

export type AppComponents = Record<
  | 'CompactMiniappCard'
  | 'MiniappMessageCard'
  | 'MiniappRail'
  | 'MiniappTimelineActivityView'
  | 'Home'
  | 'AppCardEditActions'
  | 'NativeHomeActions',
  (props: any) => Element
>;
export type Element = { type: string | ((props: any) => unknown); props: Record<string, any> };
export const sources = {
  react: ['useState', 'useRef', 'useCallback', 'useLayoutEffect'],
  'react-native': ['View', 'Text', 'Pressable', 'ActivityIndicator', 'useWindowDimensions'],
  'expo-router': ['router'],
  '@expo/ui': ['Host'],
  '@expo/ui/swift-ui': ['Button', 'Menu'],
  '@expo/ui/swift-ui/modifiers': ['buttonStyle', 'frame', 'labelStyle'],
  'expo-image': ['Image'],
  'react-native-reanimated': ['useAnimatedRef'],
  primitives: ['Icon', 'EmptyState', 'NativeButton', 'IconButton'],
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
export const aliases: Record<string, string> = {
  '../components/primitives': 'primitives',
  './primitives': 'primitives',
  './message-interaction': 'interaction',
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
export const compiled = await build({
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

export function item(template: string, extra: Record<string, unknown> = {}): ItemDetail {
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
export function fixture(
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
    primitives: {
      Icon: 'Icon',
      EmptyState: 'EmptyState',
      NativeButton: 'NativeButton',
      IconButton: 'IconButton',
    },
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
        editing: options.stateValues?.[0] === 'owner',
        editSaving: false,
        beginEdit() {},
        discardEdit() {},
        commitEdit: async () => {},
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
export function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const element = value as Element;
  if (typeof element.type === 'function') return nodes(element.type(element.props));
  return [element, ...nodes(element.props.children)];
}
export const style = (node: Element) =>
  typeof node.props.style === 'function' ? node.props.style({ pressed: false }) : node.props.style;
export const cardFor = (tree: Element, id: string) =>
  nodes(tree).find((n) => n.props.testID === `miniapp-card-${id}`)!;
