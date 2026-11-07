import type { useMessageDraft } from './use-message-draft';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as homeAccess from '../home/access.ts';
import { hookHost } from './message-selection-react.fixtures.ts';
import type { TimelineEntry } from './types';

export type Element = { type: string; props: Record<string, unknown> };
const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../app/thread/[space]/[root].tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  jsx: 'automatic',
  bundle: true,
  external: ['*'],
  platform: 'node',
  format: 'cjs',
  write: false,
});
export function find(value: unknown, type: string): Element | undefined {
  if (!value || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    for (const child of value) {
      const found = find(child, type);
      if (found) return found;
    }
    return;
  }
  const element = value as Element;
  return element.type === type ? element : find(element.props?.children, type);
}
export function fixture(
  active = true,
  jump?: string,
  individual = false,
  send: () => Promise<void> = async () => {
    throw new Error('original send failure');
  },
  draftOverride?: ReturnType<typeof useMessageDraft>,
) {
  const host = hookHost();
  let sessionEpoch = 1;
  const calls: string[] = [];
  const routes: unknown[] = [];
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      me: { id: 'alice' },
      account: {
        identity_id: 'alice',
        device_id: 'phone',
        unlocked: true,
        setup_pending: false,
      },
      conversations: [{ id: 'source', members: [{ id: 'alice' }] }],
    },
  };
  const entries = ['original', 'reply'].map(
    (id) => ({ id, kind: { Message: { text: id, card: null } } }) as TimelineEntry,
  );
  const toggle = (entry: TimelineEntry) => calls.push(`toggle:${entry.id}`);
  const selection = {
    active,
    ids: new Set(['original']),
    count: 1,
    limitReached: true,
    canCopy: true,
    canForward: true,
    canDelete: true,
    pending: false,
    remove: () => calls.push('delete'),
    toggle,
    cancel: () => calls.push('cancel'),
    copy: () => calls.push('copy'),
    forward: () => calls.push('forward'),
  };
  let routeRoot = 'root-message';
  let hookArgs: unknown[] = [];
  let actionArgs: { disabled: boolean; onSelect: (entry: TimelineEntry) => void } | undefined;
  const actions = {
    busy: false,
    panel: null,
    reply() {},
    thread() {},
    profile() {},
    open() {},
    react() {},
  };
  const module = { exports: {} as { default(): unknown } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name === 'react') return host.react;
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: string, props: Record<string, unknown>) => ({ type, props }),
          jsxs: (type: string, props: Record<string, unknown>) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (name === 'expo-router')
        return {
          router: { push: (route: unknown) => routes.push(JSON.parse(JSON.stringify(route))) },
          Stack: { Screen: 'Screen' },
          useFocusEffect: host.useFocusEffect,
          useLocalSearchParams: () => ({ space: 'space', root: routeRoot, jump }),
        };
      if (name === 'expo-router/react-navigation') return { useHeaderHeight: () => 44 };
      if (name === 'react-native-safe-area-context')
        return { useSafeAreaInsets: () => ({ top: 20 }) };
      if (name === '@shopify/flash-list') return { FlashList: 'FlashList' };
      if (name === 'react-native') return { Text: 'Text', View: 'View' };
      if (name.endsWith('/i18n/index.ts')) return { t: (key: string) => key };
      if (name.endsWith('/core/query'))
        return { useCoreQuery: () => ({ data: entries, error: null, retry() {} }) };
      if (name.endsWith('/core/provider'))
        return {
          useClient: () => ({
            store: { sessionEpoch, setTyping() {}, getSnapshot: () => state },
            snapshot: { conversations: [{ id: 'space', kind: 'Group' }] },
            mode: 'account',
          }),
        };
      if (name.endsWith('/home/access')) return homeAccess;
      if (name.endsWith('/message-audio-provider')) return { useMessageAudioFocus() {} };
      if (name.endsWith('/quote-layout-transition'))
        return { QuoteLayoutTransition: 'QuoteLayoutTransition' };
      if (name.endsWith('/quote-layout-context')) return { QuoteMotionView: 'QuoteMotionView' };
      if (name.endsWith('/mosaic-layout')) return { supportsMediaMosaic: true };
      if (name.endsWith('/core/presentation')) return { messageText: () => '' };
      if (name.endsWith('/message-presentation')) return { messageCard: () => null };
      if (name.endsWith('/conversation-timeline'))
        return {
          projectConversationTimeline: () =>
            individual
              ? entries.map((entry) => ({ key: entry.id, entry }))
              : [{ key: 'album', entry: entries[0], album: { entries } }],
        };
      if (name.endsWith('/use-message-draft'))
        return {
          useMessageDraft: () =>
            draftOverride ?? {
              messageDraft: { text: 'draft', reply: null, editing: null, editText: '' },
              updateDraft() {},
              capture: () => ({ text: 'draft' }),
              isCurrent: () => true,
              send,
            },
        };
      if (name.endsWith('/primitives'))
        return { EmptyState: 'Empty', Loading: 'Loading', NativeButton: 'Button' };
      if (name.endsWith('/theme')) return { useTheme: () => ({ colors: {} }) };
      if (name.endsWith('/home/provider')) return { useHome: () => ({ allItems: [] }) };
      if (name.endsWith('/thread-hashtag-provider'))
        return { ThreadHashtagProvider: 'ThreadHashtagProvider' };
      if (name.endsWith('/message-composer')) return { MessageComposer: 'Composer' };
      if (name.endsWith('/use-message-selection'))
        return {
          useMessageSelection: (...args: unknown[]) => {
            hookArgs = args;
            return selection;
          },
        };
      if (name.endsWith('/message-selection-toolbar'))
        return {
          MessageSelectionToolbar: 'SelectionToolbar',
          MessageSelectionHeader: 'SelectionHeader',
          MessageSelectionCancel: 'SelectionCancel',
        };
      if (name.endsWith('/conversation-viewport'))
        return { ConversationContainer: 'Container', ConversationViewport: 'Viewport' };
      if (name.endsWith('/conversation-timeline-message'))
        return { ConversationTimelineMessage: 'Message' };
      if (name.endsWith('/use-message-actions'))
        return {
          useMessageActions: (args: typeof actionArgs) => {
            actionArgs = args;
            return actions;
          },
        };
      throw new Error(name);
    },
  });
  const render = () => host.render(() => module.exports.default());
  const tree = render();
  const viewport = find(tree, 'Viewport')!;
  const content = (viewport.props.children as (spacer: unknown, scroll: unknown) => Element)(
    null,
    'Scroll',
  );
  const transition = find(content, 'QuoteLayoutTransition');
  assert.ok(transition, 'Thread viewport must retain its shared quote transition');
  const transitioned = (transition.props.children as (...args: unknown[]) => Element)(
    false,
    () => {},
    () => {},
    false,
  );
  const list = find(transitioned, 'FlashList');
  assert.ok(list, 'Thread viewport must retain its actual list');
  return {
    calls,
    routes,
    state,
    entries,
    selection,
    tree,
    viewport,
    list,
    hookArgs,
    actionArgs,
    render,
    setRoot: (root: string) => {
      routeRoot = root;
    },
    reopen: () => {
      sessionEpoch++;
    },
  };
}
