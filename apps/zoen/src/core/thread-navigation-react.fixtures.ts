import assert from 'node:assert/strict';
import * as React from 'react';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { plainMessage } from './message-caption.fixtures.ts';
import type { TimelineEntry } from './types';
import { find, type Element } from './thread-message-selection.fixtures.ts';

const require = createRequire(import.meta.url);
const views = new Set([
  'thread-hashtag-provider',
  'quote-layout-context',
  'quote-layout-transition',
  'message-composer',
  'message-selection-toolbar',
  'conversation-viewport',
  'conversation-timeline-message',
]);
const bundle = await build({
  entryPoints: [new URL('../app/thread/[space]/[root].tsx', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'compiled-thread-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          const key = path.split('/').at(-1)!;
          return [
            'react',
            'react/compiler-runtime',
            'react/jsx-runtime',
            'expo-router',
            'expo-router/react-navigation',
            'react-native',
            'react-native-safe-area-context',
            '@shopify/flash-list',
          ].includes(path) ||
            views.has(key) ||
            /\/(?:core\/provider|core\/query|home\/provider|theme|primitives|navigation-container|keyboard-motion|mosaic-layout|message-audio-provider|use-message-draft|use-message-actions|use-message-selection|i18n\/index.ts)$/.test(
              path,
            )
            ? { path, external: true }
            : undefined;
        });
        builder.onLoad({ filter: /\/thread\/\[space\]\/\[root\]\.tsx$/ }, ({ path }) => ({
          contents: compileExpoSource(path, true),
          loader: 'js',
        }));
        builder.onLoad(
          {
            filter:
              /\/(?:use-thread-message-navigation|use-conversation-message-navigation|use-chat-native-metrics)\.ts$/,
          },
          ({ path }) => ({ contents: compileExpoSource(path, true), loader: 'js' }),
        );
      },
    },
  ],
});

/** Real React, ClientStore and optimized navigation hooks; native geometry/IO are explicit ports. */
export async function threadNavigationFixture(jump?: string, album = false) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const space = snapshot.conversations[0]!.id;
  let routeRoot = 'root',
    focused = true,
    disabled = false;
  const person = snapshot.conversations[0]!.members[0]!;
  const message = (id: string, parent: string | null, seq: string): TimelineEntry => ({
    ...plainMessage(id),
    id,
    text_revision: `revision-${id}`,
    in_thread: parent,
    seq,
    author: person,
  });
  const entries = [
    message('root', null, '1'),
    message('source', 'root', '2'),
    message('latest', 'root', '3'),
  ];
  if (album)
    for (const [index, entry] of entries.slice(1).entries()) {
      entry.kind = {
        Message: {
          text: entry.id,
          card: {
            item_id: `file-${index}`,
            kind_id: 'file',
            summary: '',
            kind_label: 'File',
            total_cents: null,
            budget_cents: null,
            line_count: 0,
            done_count: 0,
            title: 'Photo',
            version: 1,
            app: null,
          },
        },
      };
      entry.media = {
        presentation: 'media',
        group: { id: `album_${'a'.repeat(64)}`, index, count: 2 },
      };
    }
  const otherEntries = [
    message('different-root', null, '1'),
    message('other-child', 'different-root', '2'),
  ];
  const calls: unknown[] = [],
    routes: unknown[] = [],
    offsets: number[] = [],
    indices: unknown[] = [];
  let read = async (id: string): Promise<TimelineEntry> =>
    message(id, id === 'elsewhere' ? 'another-root' : null, '1');
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'set_typing') return undefined;
    calls.push(command);
    if (command.op === 'message') return read(command.message_id);
    throw Error(`Unexpected Thread command ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const Focus = React.createContext(true);
  const useFocusPort = (work: () => void | (() => void)) => {
    const active = React.useContext(Focus);
    React.useEffect(() => (active ? work() : undefined), [active, work]);
  };
  const draft = {
    messageDraft: {
      text: 'Private unsent **thread** answer',
      reply: null,
      editing: null,
      editText: '',
      editBaseText: '',
    },
    updateDraft() {},
    ready: true,
    error: null,
    retry() {},
    flush() {},
    replyTo: null,
  };
  const items: never[] = [];
  let frameSerial = 0;
  const frames = new Map<number, () => void>();
  const module = { exports: {} as { default(): Element } };
  runInNewContext(bundle.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    AbortController,
    process: { env: { EXPO_OS: 'ios' } },
    requestAnimationFrame: (work: () => void) => {
      frames.set(++frameSerial, work);
      return frameSerial;
    },
    cancelAnimationFrame: (id: number) => {
      frames.delete(id);
    },
    require(path: string) {
      if (path === 'react') return React;
      if (path === 'react/compiler-runtime' || path === 'react/jsx-runtime') return require(path);
      if (path === 'expo-router')
        return {
          Stack: { Screen: 'Screen' },
          router: { push: (value: unknown) => routes.push(structuredClone(value)) },
          useLocalSearchParams: () => ({ space, root: routeRoot, jump }),
          useFocusEffect: useFocusPort,
          useIsFocused: () => React.useContext(Focus),
        };
      if (path === 'expo-router/react-navigation') return { useHeaderHeight: () => 64 };
      if (path === 'react-native-safe-area-context')
        return { useSafeAreaInsets: () => ({ top: 20 }) };
      if (path === 'react-native')
        return {
          Text: 'Text',
          View: 'View',
          AppState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
        };
      if (path === '@shopify/flash-list') return { FlashList: 'FlashList' };
      if (path.endsWith('/core/provider'))
        return {
          useClient: () => ({
            store,
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
          }),
        };
      if (path.endsWith('/core/query'))
        return {
          useCoreQuery: () => ({
            data: routeRoot === 'root' ? entries : otherEntries,
            error: null,
            retry() {},
          }),
        };
      if (path.endsWith('/home/provider')) return { useHome: () => ({ allItems: items }) };
      if (path.endsWith('/theme'))
        return {
          useTheme: () => ({
            colors: { text: '#fff' },
            presentation: { navigation: { text: '#fff' } },
          }),
        };
      if (path.endsWith('/i18n/index.ts')) return { t: (key: string) => key };
      if (path.endsWith('/navigation-container'))
        return { NavigationContainer: 'NavigationContainer', navigationMetricsAvailable: true };
      if (path.endsWith('/keyboard-motion'))
        return { useKeyboardMotion: () => ({ height: { value: 0 }, progress: { value: 0 } }) };
      if (path.endsWith('/mosaic-layout')) return { supportsMediaMosaic: () => album };
      if (path.endsWith('/message-audio-provider')) return { useMessageAudioFocus() {} };
      if (path.endsWith('/use-message-draft')) return { useMessageDraft: () => draft };
      if (path.endsWith('/use-message-actions'))
        return { useMessageActions: () => ({ busy: false }) };
      if (path.endsWith('/use-message-selection'))
        return { useMessageSelection: () => ({ active: disabled, ids: new Set(), count: 0 }) };
      if (path.endsWith('/primitives'))
        return { EmptyState: 'Empty', Loading: 'Loading', NativeButton: 'Button' };
      const names: Record<string, string[]> = {
        'thread-hashtag-provider': ['ThreadHashtagProvider'],
        'quote-layout-context': ['QuoteMotionView'],
        'quote-layout-transition': ['QuoteLayoutTransition'],
        'message-composer': ['MessageComposer'],
        'message-selection-toolbar': [
          'MessageSelectionHeader',
          'MessageSelectionToolbar',
          'MessageSelectionCancel',
        ],
        'conversation-viewport': ['ConversationContainer', 'ConversationViewport'],
        'conversation-timeline-message': ['ConversationTimelineMessage'],
      };
      const key = path.split('/').at(-1)!;
      assert.ok(names[key], `Unexpected native Thread port: ${path}`);
      return Object.fromEntries(names[key]!.map((name) => [name, name]));
    },
  });
  let tree: Element;
  function Surface() {
    tree = module.exports.default();
    return null;
  }
  const render = () =>
    root.render(
      React.createElement(Focus.Provider, { value: focused }, React.createElement(Surface)),
    );
  const ui = () => {
    const viewport = find(tree, 'ConversationViewport')!,
      header = find(tree, 'Screen')!;
    const transition = find(
      (viewport.props.children as any)(null, 'Scroll'),
      'QuoteLayoutTransition',
    )!;
    const list = find(
      (transition.props.children as any)(
        false,
        () => {},
        () => {},
        false,
      ),
      'FlashList',
    )!;
    return {
      viewport: viewport.props as any,
      list: list.props as any,
      header: (header.props.options as any).headerTitle().props as any,
      message(index = 0) {
        return find(
          (list.props.renderItem as any)({ item: (list.props.data as unknown[])[index], index }),
          'ConversationTimelineMessage',
        )!.props as any;
      },
    };
  };
  await render();
  let sequence = 0;
  let geometry = {
    contentOffset: { y: 0 },
    contentSize: { height: 1800 },
    layoutMeasurement: { height: 700 },
    contentInset: { top: 0, bottom: 300 },
  };
  const packet = (binding: string) => ({
    binding,
    sequence: String(++sequence),
    geometry: structuredClone(geometry),
  });
  ui().list.ref.current = {
    getLayout: (index: number) => ({
      x: 0,
      y: index ? index * 600 : 72,
      width: 402,
      height: 100,
      isHeightMeasured: true,
      isWidthMeasured: true,
    }),
    getScrollableNode: () => 42,
    getFirstItemOffset: () => 0,
    getChildContainerDimensions: () => ({ width: 402, height: 1800 }),
    getWindowSize: () => ({ width: 402, height: 700 }),
    scrollToOffset: ({ offset }: { offset: number }) => {
      offsets.push(offset);
      geometry = { ...geometry, contentOffset: { y: offset } };
    },
    scrollToIndex: (request: unknown) => {
      indices.push(request);
      return Promise.resolve();
    },
  };
  const attach = () => {
    ui().header.metrics.ref.current = { sampleScroll: async (binding: string) => packet(binding) };
  };
  attach();
  await root.act(() => ui().list.onLoad());
  const commit = () =>
    root.act(() => {
      ui().viewport.onNativeLayout(700);
      ui().viewport.onNativeInset(geometry.contentInset);
      ui().list.onContentSizeChange(402, 1800);
      ui().list.onCommitLayoutEffect();
      for (let i = 0; i < ui().list.data.length; ++i)
        ui()
          .message(i)
          .onRevealLayout?.(ui().list.data[i].entry.id, { y: 0, width: 402, height: 100 });
    });
  return {
    store,
    snapshot,
    root,
    ui,
    calls,
    routes,
    offsets,
    indices,
    draft,
    entries,
    commit,
    frame: () =>
      root.act(() => {
        const batch = [...frames.values()];
        frames.clear();
        for (const work of batch) work();
      }),
    tap: (id: string, space?: string) => root.act(() => ui().message().onQuote(id, space)),
    read: (next: typeof read) => {
      read = next;
    },
    async keyboard(bottom: number) {
      geometry = { ...geometry, contentInset: { top: 0, bottom } };
      await root.act(() => ui().header.metrics.changed(packet(ui().header.metrics.binding)));
      await commit();
    },
    async changeRoot() {
      routeRoot = 'different-root';
      await render();
      attach();
    },
    async selection(value: boolean) {
      disabled = value;
      await render();
    },
    async blur() {
      focused = false;
      await render();
    },
    async focus() {
      focused = true;
      await render();
      attach();
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
