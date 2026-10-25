import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import { dirname } from 'node:path';
import { HomeController, projectHomeItems } from '../home/preferences.ts';
import {
  projectConversationTimeline,
  type ConversationTimelineRow,
} from './conversation-timeline.ts';
import { messageCard } from './message-presentation.ts';
import { conversationScope } from './conversation-scope.ts';
import { conversationRevealContent } from './conversation-reveal-content.ts';
import * as binaryFormats from '../miniapps/binary-formats.ts';
import { selectedDeckHead } from '../miniapps/deck-version-selection.ts';
import type { ItemDetail, TimelineEntry } from './types';
import type { ItemCard } from './generated/ItemCard';

type Element = { type: string; props: Record<string, any> };
const sourceRoot = new URL('../', import.meta.url);
const sources = {
  Chat: 'app/chat/[id].tsx',
  Thread: 'app/thread/[space]/[root].tsx',
  MiniappScreen: 'miniapps/detail-screen.tsx',
};
const compiled = Object.fromEntries(
  await Promise.all(
    Object.entries(sources).map(async ([name, relative]) => {
      const path = new URL(relative, sourceRoot);
      const result = await build({
        stdin: {
          contents: readFileSync(path, 'utf8'),
          resolveDir: dirname(fileURLToPath(path)),
          loader: 'tsx',
        },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        define: { 'process.env.EXPO_OS': '"ios"' },
        plugins: [
          {
            name: 'actual-chat-owner-and-view',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path }) =>
                /\/(?:chat-screen|use-chat-controller|use-conversation-search)$/.test(path)
                  ? undefined
                  : { path, external: true },
              );
            },
          },
        ],
      });
      return [name, result.outputFiles[0]!.text];
    }),
  ),
);
const canonical = {
  id: 'app_drawing',
  space_id: 'chat',
  kind_id: 'app',
  version: 3,
  title: 'Current authorized drawing',
  app: { app_id: 'excalidraw', headline: 'Current drawing', snapshot_json: '{"version":3}' },
} as ItemDetail;
const embedded = {
  item_id: canonical.id,
  kind_id: 'app',
  version: 1,
  title: 'Old shared drawing',
  app: { ...canonical.app, headline: 'Old drawing', snapshot_json: '{"version":1}' },
} as ItemCard;
function entries(thread: boolean): TimelineEntry[] {
  const common = {
    author: { id: 'alice', name: 'Alice', is_me: true } as TimelineEntry['author'],
    delivery: 'Sent' as const,
    reply_to: null,
    in_thread: thread ? 'thread_root' : null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
  };
  return [
    {
      ...common,
      id: 'shared',
      seq: '1',
      at_ms: '1000',
      kind: { Message: { card: embedded, text: '' } },
    },
    ...[2, 3].map((version) => ({
      ...common,
      id: `edit_${version}`,
      seq: `${version}`,
      at_ms: `${version * 1000}`,
      kind: {
        ItemEdited: {
          item_id: canonical.id,
          title: 'Drawing',
          version,
          note: 'Updated',
          is_undo: false,
        },
      },
    })),
  ] as TimelineEntry[];
}
async function fixture() {
  const saved = new Map<string, string>();
  const controller = new HomeController(
    {
      owner: () => ({ mode: 'account', identityId: 'alice', deviceId: 'phone' }),
      spaces: () => new Set(['chat']),
      items: async () => [canonical],
    },
    {
      get: async (key) => saved.get(key) ?? null,
      set: async (key, value) => {
        saved.set(key, value);
      },
    },
  );
  await controller.activate();
  await controller.hide(canonical.id);
  function home() {
    const state = controller.getSnapshot();
    return {
      ...projectHomeItems(state.items, state),
      ownerKey: 'alice-phone',
      pinnedItems: [],
      pinnedIds: state.pinnedIds,
      loading: state.loading,
      error: state.error,
      isPinned: (id: string) => state.pinnedIds.includes(id),
      pin: (id: string) => controller.pin(id),
      unpin: (id: string) => controller.unpin(id),
    };
  }
  return { controller, home };
}

/** Actual screen bodies, real preference/timeline logic, host-only UI/hook ports.
 * Effects and native rendering are intentionally outside this local regression. */
function renderScreen(
  name: keyof typeof sources,
  home: ReturnType<Awaited<ReturnType<typeof fixture>>['home']>,
  timelineOverride?: TimelineEntry[],
  mosaic = false,
) {
  const timeline = timelineOverride ?? entries(name === 'Thread');
  const snapshot = {
    me: { id: 'alice' },
    account: { identity_id: 'alice', device_id: 'phone', unlocked: true, setup_pending: false },
    conversations: [{ id: 'chat', kind: 'Direct', title: 'Alice', members: [{ id: 'alice' }] }],
  };
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({ phase: 'ready', mode: 'account', snapshot }),
  };
  const draft = {
    messageDraft: { text: '', reply: null, editing: null, editText: '' },
    replyTo: null,
  };
  const noop = () => {};
  class SearchPort {
    subscribe = noop;
    getSnapshot = () => ({ active: false, hits: [], index: 0, query: '' });
  }
  class LayoutPort {
    scope = 'chat:alice-phone';
    cancel = noop;
  }
  const components = (names: string[]) => Object.fromEntries(names.map((key) => [key, key]));
  function port(path: string): unknown {
    if (path === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' };
    if (path === 'react')
      return {
        useState: (value: unknown) => [typeof value === 'function' ? value() : value, noop],
        useRef: (value: unknown) => ({ current: value }),
        useMemo: (value: () => unknown) => value(),
        useCallback: (value: unknown) => value,
        useEffect: noop,
        useLayoutEffect: noop,
        useSyncExternalStore: (_subscribe: unknown, get: () => unknown) => get(),
      };
    if (path === 'expo-router')
      return {
        router: { push: noop, replace: noop, back: noop },
        Stack: { Screen: 'StackScreen' },
        useLocalSearchParams: () => ({
          id: name === 'MiniappScreen' ? canonical.id : 'chat',
          space: 'chat',
          root: 'thread_root',
        }),
        useFocusEffect: noop,
        useIsFocused: () => true,
        usePreventRemove: () => noop,
      };
    if (path === 'expo-router/react-navigation') return { useHeaderHeight: () => 96 };
    if (path === 'react-native-safe-area-context')
      return { useSafeAreaInsets: () => ({ top: 48 }) };
    if (path === '@shopify/flash-list') return { FlashList: 'FlashList' };
    if (path === 'react-native') return components(['Pressable', 'Text', 'View', 'ScrollView']);
    if (path.endsWith('/i18n/index.ts')) return { t: (key: string) => key };
    if (path.endsWith('/theme'))
      return { useTheme: () => ({ colors: { text: '#111', secondary: '#555' } }) };
    if (path.endsWith('/home/provider')) return { useHome: () => home };
    if (path.endsWith('/core/provider') || path === './provider')
      return {
        useClient: () => ({
          store,
          snapshot,
          timelines: { chat: { entries: timeline, has_more: false } },
          activity: {},
          notice: null,
          mode: 'account',
          phase: 'ready',
          erasePending: false,
        }),
      };
    if (path.endsWith('/core/query'))
      return {
        useCoreQuery: (query: { op: string }) => ({
          data: query.op === 'thread' ? timeline : query.op === 'item' ? canonical : [],
          error: null,
          retry: noop,
        }),
      };
    if (path.endsWith('/core/conversation-timeline')) return { projectConversationTimeline };
    if (path.endsWith('/core/message-presentation')) return { messageCard };
    if (path.endsWith('/core/presentation')) return { messageText: () => 'Drawing update' };
    if (path.endsWith('/core/use-message-draft')) return { useMessageDraft: () => draft };
    if (path.endsWith('/core/conversation-search') || path === './conversation-search')
      return { ConversationSearch: SearchPort };
    if (path.endsWith('/core/conversation-scope') || path === './conversation-scope')
      return { conversationScope };
    if (path.endsWith('/core/initial-timeline-end')) return { InitialTimelineEnd: LayoutPort };
    if (path === './navigation-container') return { navigationMetricsAvailable: false };
    if (path === './use-chat-native-metrics')
      return { useChatNativeMetrics: () => ({ navigationMetrics: {}, observe: undefined }) };
    if (path.endsWith('/core/conversation-reveal')) return { ConversationReveal: LayoutPort };
    if (path.endsWith('/core/conversation-reveal-content')) return { conversationRevealContent };
    if (path.endsWith('/core/live-activity')) return { chatActivityLabel: () => '' };
    if (path.endsWith('/components/keyboard-motion'))
      return { useKeyboardMotion: () => ({ progress: { value: 0 } }) };
    if (path.endsWith('/components/use-message-actions'))
      return { useMessageActions: () => ({ busy: false }) };
    if (path.endsWith('/components/dialogs')) return { useDialogs: () => ({ inform: noop }) };
    if (path.endsWith('/media/message-audio-provider')) return { useMessageAudioFocus: noop };
    if (path.endsWith('/components/primitives'))
      return components(['EmptyState', 'Loading', 'NativeButton', 'Surface']);
    if (path.endsWith('/components/message-composer'))
      return { MessageComposer: 'MessageComposer' };
    if (path.endsWith('/components/conversation-viewport'))
      return components(['ConversationContainer', 'ConversationViewport']);
    if (path.endsWith('/components/conversation-rail'))
      return components(['ConversationRail', 'ConversationRailSpacer']);
    if (path.endsWith('/components/conversation-timeline-message'))
      return { ConversationTimelineMessage: 'ConversationMessage' };
    if (path.endsWith('/components/chat-header')) return { ChatHeader: 'ChatHeader' };
    if (path.endsWith('/miniapps/rail')) return { MiniappRail: 'MiniappRail' };
    if (path.endsWith('/conversation-provider'))
      return {
        useConversationCards: () => ({
          items: [canonical],
          visible: [],
          loading: false,
          isPinned: () => true,
        }),
      };
    if (path.endsWith('/home/menus')) return { HomeCardMenu: 'HomeCardMenu' };
    if (path === './deck-version-selection') return { selectedDeckHead };
    if (path === './state-view') return { MiniappState: 'MiniappState' };
    if (path === './view-host') return { MiniappView: 'MiniappView' };
    if (path === './editor-agent-sheet') return { EditorAgentAccess: 'EditorAgentAccess' };
    if (path === './binary-formats') return binaryFormats;
    if (path.endsWith('/native-surfaces/controls'))
      return { MiniappSurfaceControls: 'MiniappSurfaceControls' };
    if (path === './actions' || path.endsWith('/assistant/launch')) return {};
    if (path.endsWith('/media/mosaic-layout')) return { supportsMediaMosaic: mosaic };
    return assert.fail(`Missing screen port: ${path}`);
  }
  const jsx = (type: string, props: Element['props']) => ({ type, props });
  const module = { exports: {} as Record<string, (props?: unknown) => Element> };
  runInNewContext(compiled[name]!, {
    module,
    exports: module.exports,
    require: port,
    process: { env: {} },
  });
  return name === 'MiniappScreen'
    ? module.exports.MiniappScreen!({ manage: true })
    : module.exports.default!();
}
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  const children =
    node.type === 'ConversationViewport' && typeof node.props.children === 'function'
      ? node.props.children('ComposerSpacer', 'NativeScroll')
      : node.props.children;
  return [node, ...nodes(children), ...nodes(node.props.overlay)];
}

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
