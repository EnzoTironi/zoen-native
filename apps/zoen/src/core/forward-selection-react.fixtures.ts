import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { hookHost } from './message-selection-react.fixtures.ts';
import { draftSession } from './store.fixtures.ts';
import { conversationScope } from './conversation-scope.ts';
import { emptyActivity } from './live-activity.ts';
import * as messageSelection from './message-selection.ts';
import * as messagePresentation from './message-presentation.ts';
import * as forwardOptions from './forward-options.ts';
import * as access from '../home/access.ts';
import type { TimelineEntry } from './types';

type Element = { type: string; props: Record<string, unknown> };
export type Review = {
  targetId: string;
  ready: boolean;
  completed: boolean;
  skippedIds: string[];
  messages: Array<{
    message: TimelineEntry;
    options: { hide_author: boolean; hide_caption: boolean };
    acceptedMessageId: string | null;
    targetId: string;
  }>;
};
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../app/forward-selection/[space].tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  jsx: 'automatic',
  bundle: true,
  external: ['*'],
  platform: 'node',
  format: 'cjs',
  write: false,
});
export function all(value: unknown, match: (element: Element) => boolean): Element[] {
  if (!value || typeof value !== 'object') return [];
  if (Array.isArray(value)) return value.flatMap((child) => all(child, match));
  const element = value as Element;
  return [...(match(element) ? [element] : []), ...all(element.props?.children, match)];
}
export function text(value: unknown): string {
  return Array.isArray(value) ? value.map(text).join('') : typeof value === 'string' ? value : '';
}
export function fixture() {
  const host = hookHost();
  const snapshot = draftSession();
  const source = snapshot.conversations[0]!;
  let state = {
    phase: 'ready' as const,
    mode: 'account' as const,
    erasePending: false,
    timelines: {},
    notice: null,
    activity: emptyActivity(),
    snapshot: {
      ...snapshot,
      conversations: [
        source,
        { ...source, id: 'target', title: 'Recipient', can_write: true },
        { ...source, id: 'other', title: 'Other Recipient', can_write: true },
      ],
    },
  };
  const listeners = new Set<() => void>();
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    driver: {
      request: async (command: unknown) => {
        calls.push({ action: 'driver', value: command });
      },
    },
    request: async (command: unknown) => {
      calls.push({ action: 'store', value: command });
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  const calls: Array<{ action: string; value?: unknown }> = [];
  const sessions: MockSession[] = [];
  class MockSession {
    get configurable() {
      return typeof this.port.configure === 'function';
    }
    readonly scope: unknown;
    readonly port: Record<string, (...args: unknown[]) => unknown>;
    readonly callbacks: { reviewed(value: Review | null): void; busy(value: boolean): void };
    constructor(
      scope: unknown,
      port: Record<string, (...args: unknown[]) => unknown>,
      callbacks: { reviewed(value: Review | null): void; busy(value: boolean): void },
    ) {
      this.scope = scope;
      this.port = port;
      this.callbacks = callbacks;
      sessions.push(this);
    }
    async focus() {
      calls.push({ action: 'focus' });
    }
    blur() {
      calls.push({ action: 'blur' });
    }
    dispose() {}
    refresh() {}
    clearReview() {
      calls.push({ action: 'clear' });
    }
    async prepare(target: string) {
      calls.push({ action: 'prepare', value: target });
    }
    async confirm(review: Review) {
      calls.push({ action: 'confirm', value: review });
    }
    async configure(review: Review, options: unknown) {
      calls.push({ action: 'configure', value: { review, options } });
    }
    async finish(review: Review) {
      calls.push({ action: 'finish', value: review });
    }
    async abandon(review: Review) {
      calls.push({ action: 'abandon', value: review });
    }
  }
  const params = {
    space: source.id,
    messages: '["a","b"]',
    selectionScope: conversationScope(state, 1, source.id),
  };
  const module = { exports: {} as { default(): unknown } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    process: { env: { EXPO_OS: 'ios' } },
    exports: module.exports,
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
          Stack: { Screen: 'Screen' },
          router: { back: () => calls.push({ action: 'back' }) },
          useFocusEffect: host.useFocusEffect,
          useLocalSearchParams: () => params,
        };
      if (name === 'expo-router/react-navigation') return { useHeaderHeight: () => 44 };
      if (name === 'react-native-safe-area-context')
        return { useSafeAreaInsets: () => ({ bottom: 0 }) };
      if (name === 'react-native')
        return {
          View: 'View',
          Text: 'Text',
          TextInput: 'TextInput',
          Pressable: 'Pressable',
          ScrollView: 'ScrollView',
          Keyboard: { dismiss() {} },
        };
      if (name === '@shopify/flash-list') return { FlashList: 'FlashList' };
      if (name.endsWith('/conversation-scope')) return { conversationScope };
      if (name.endsWith('/message-selection')) return messageSelection;
      if (name.endsWith('/message-presentation')) return messagePresentation;
      if (name.endsWith('/forward-options')) return forwardOptions;
      if (name.endsWith('/forward-options-menu')) return { ForwardOptionsMenu: 'Options' };
      if (name.endsWith('/forward-selection')) return { ForwardSelectionSession: MockSession };
      if (name.endsWith('/i18n/index.ts'))
        return {
          t: (key: string, values?: { accepted?: number; count: number }) =>
            values
              ? values.accepted === undefined
                ? `${key}:${values.count}`
                : `${key}:${values.accepted}/${values.count}`
              : key,
          formatLocale: 'en',
        };
      if (name.endsWith('/conversation-avatar')) return { ConversationAvatar: 'Avatar' };
      if (name.endsWith('/presentation'))
        return { conversationTitle: (chat: { title?: string }) => chat.title ?? 'Source' };
      if (name.endsWith('/forward-message-preview')) return { ForwardMessagePreview: 'Preview' };
      if (name.endsWith('/provider')) return { useClient: () => ({ ...state, store }) };
      if (name.endsWith('/home/access')) return access;
      if (name.endsWith('/theme')) return { useTheme: () => ({ colors: {} }) };
      if (name.endsWith('/primitives')) return { EmptyState: 'Empty', NativeButton: 'Button' };
      throw new Error(name);
    },
  });
  return {
    calls,
    store,
    render: () => host.render(() => module.exports.default()),
    blur: host.blur,
    ports: () => sessions.at(-1)!.port,
    publish: (review: Review) => sessions.at(-1)!.callbacks.reviewed(review),
    busy: () => sessions.at(-1)!.callbacks.busy(true),
    revoke: () => {
      state = { ...state, snapshot: { ...state.snapshot, conversations: [] } };
      for (const listener of listeners) listener();
    },
    choose(tree: unknown, targetId = 'target') {
      const list = all(tree, (element) => element.type === 'FlashList')[0]!;
      const item = (list.props.data as Array<{ id: string }>).find((chat) => chat.id === targetId)!;
      const row = (list.props.renderItem as (value: { item: unknown }) => Element)({ item });
      (row.props.onPress as () => void)();
    },
  };
}
export const review = (accepted = 0, ready = true, completed = false): Review => ({
  targetId: 'target',
  ready,
  completed,
  skippedIds: [],
  messages: ['a', 'b'].map((id, index) => ({
    message: {
      id,
      author: { name: id },
      kind: { Message: { text: id, card: null } },
    } as TimelineEntry,
    targetId: 'target',
    options: { hide_author: false, hide_caption: false },
    acceptedMessageId: index < accepted ? `accepted-${id}` : null,
  })),
});
