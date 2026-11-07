import * as messageLinks from './message-link.ts';
import assert from 'node:assert/strict';
import { createContext } from 'react';
import { conversationTitle } from '../saved-messages/presentation.ts';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { dirname } from 'node:path';
import { HomeController, projectHomeItems } from '../home/preferences.ts';
import * as homeAccess from '../home/access.ts';
import { projectConversationTimeline } from './conversation-timeline.ts';
import { messageCard } from './message-presentation.ts';
import { conversationScope } from './conversation-scope.ts';
import { conversationRevealContent } from './conversation-reveal-content.ts';
import * as binaryFormats from '../miniapps/binary-formats.ts';
import { selectedDeckHead } from '../miniapps/deck-version-selection.ts';
import * as appReview from '../miniapps/app-request-review.ts';
import type { ItemDetail, TimelineEntry } from './types';
import type { ItemCard } from './generated/ItemCard';

export type Element = { type: string; props: Record<string, any> };
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
                /\/(?:chat-screen|use-chat-controller|use-conversation-message-navigation|use-thread-message-navigation|use-conversation-search|use-reply-destination-focus|conversation-hashtag-search|conversation-hashtag-provider|message-hashtag|search-launch|send-chat-message)$/.test(
                  path,
                )
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
export const canonical = {
  id: 'app_drawing',
  space_id: 'chat',
  kind_id: 'app',
  version: 3,
  title: 'Current authorized drawing',
  app: { app_id: 'excalidraw', headline: 'Current drawing', snapshot_json: '{"version":3}' },
} as ItemDetail;
export const embedded = {
  item_id: canonical.id,
  kind_id: 'app',
  version: 1,
  title: 'Old shared drawing',
  app: { ...canonical.app, headline: 'Old drawing', snapshot_json: '{"version":1}' },
} as ItemCard;
export function entries(thread: boolean): TimelineEntry[] {
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
export async function fixture() {
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
export function renderScreen(
  name: keyof typeof sources,
  home: ReturnType<Awaited<ReturnType<typeof fixture>>['home']>,
  timelineOverride?: TimelineEntry[],
  mosaic = false,
  onInitialEndCancel: () => void = () => {},
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
    cancel = onInitialEndCancel;
  }
  const components = (names: string[]) => Object.fromEntries(names.map((key) => [key, key]));
  function port(path: string): unknown {
    if (path.endsWith('/thread-hashtag-provider'))
      return { ThreadHashtagProvider: 'ThreadHashtagProvider' };
    if (path === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' };
    if (path === 'react')
      return {
        createContext,
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
    if (path.endsWith('/core/message-link')) return messageLinks;
    if (path.endsWith('/core/message-presentation')) return { messageCard };
    if (path.endsWith('/core/presentation')) return { messageText: () => 'Drawing update' };
    if (path.endsWith('/core/use-message-draft')) return { useMessageDraft: () => draft };
    if (
      path.endsWith('/core/conversation-search') ||
      path === './conversation-search' ||
      path === './conversation-search.ts'
    )
      return { ConversationSearch: SearchPort };
    if (
      path.endsWith('/core/conversation-scope') ||
      path === './conversation-scope' ||
      path === './conversation-scope.ts'
    )
      return { conversationScope };
    if (path.endsWith('/core/initial-timeline-end')) return { InitialTimelineEnd: LayoutPort };
    if (path.endsWith('/navigation-container'))
      return { navigationMetricsAvailable: false, NavigationContainer: 'NavigationContainer' };
    if (path.endsWith('/quote-layout-context')) return { QuoteMotionView: 'QuoteMotionView' };
    if (path.endsWith('/quote-layout-transition'))
      return { QuoteLayoutTransition: 'QuoteLayoutTransition' };
    if (path === './use-chat-native-metrics')
      return { useChatNativeMetrics: () => ({ navigationMetrics: {}, observe: undefined }) };
    if (path === './use-conversation-rail')
      return { useConversationRail: () => ({ visibleHeight: { value: 0 } }) };
    if (path.endsWith('/core/conversation-reveal')) return { ConversationReveal: LayoutPort };
    if (path.endsWith('/core/conversation-reveal-content')) return { conversationRevealContent };
    if (path.endsWith('/core/live-activity')) return { chatActivityLabel: () => '' };
    if (path.endsWith('/components/keyboard-motion') || path === './keyboard-motion')
      return { useKeyboardMotion: () => ({ progress: { value: 0 } }) };
    if (path.endsWith('/use-message-selection'))
      return {
        useMessageSelection: () => ({
          active: false,
          ids: new Set<string>(),
          count: 0,
          limitReached: false,
          canCopy: false,
          canForward: false,
          toggle: noop,
          cancel: noop,
          copy: noop,
          forward: noop,
        }),
      };
    if (path.endsWith('/message-selection-toolbar'))
      return components([
        'MessageSelectionToolbar',
        'MessageSelectionHeader',
        'MessageSelectionCancel',
      ]);
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
    if (path.endsWith('/saved-messages/presentation')) return { conversationTitle };
    if (path.endsWith('/components/chat-header')) return { ChatHeader: 'ChatHeader' };
    if (path.endsWith('/chat-date-navigation')) return { ChatDateNavigation: 'ChatDateNavigation' };
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
    if (path.endsWith('/home/access')) return homeAccess;
    if (path === './deck-version-selection') return { selectedDeckHead };
    if (path === './state-view') return { MiniappState: 'MiniappState' };
    if (path === './view-host') return { MiniappView: 'MiniappView' };
    if (path === './editor-agent-sheet') return { EditorAgentAccess: 'EditorAgentAccess' };
    if (path === './package-release-sheet')
      return { AppPackageReleaseSheet: 'AppPackageReleaseSheet' };
    if (path === './binary-formats') return binaryFormats;
    if (path.endsWith('/native-surfaces/controls'))
      return { MiniappSurfaceControls: 'MiniappSurfaceControls' };
    if (path === './actions' || path.endsWith('/assistant/launch')) return {};
    if (path.endsWith('/media/mosaic-layout')) return { supportsMediaMosaic: mosaic };
    if (path === './app-request-review') return appReview;
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
export function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  const children =
    node.type === 'QuoteLayoutTransition' && typeof node.props.children === 'function'
      ? node.props.children(
          false,
          () => {},
          () => {},
          false,
        )
      : node.type === 'ConversationViewport' && typeof node.props.children === 'function'
        ? node.props.children('ComposerSpacer', 'NativeScroll')
        : node.props.children;
  return [node, ...nodes(children), ...nodes(node.props.overlay)];
}
