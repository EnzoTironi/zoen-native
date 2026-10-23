import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { resolveSkyTheme } from '../theme/sky.ts';
import { messageSourceModules } from './message-source-compiler.ts';
import type { TimelineEntry } from './types';

export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
export const conversationPath = fileURLToPath(
  new URL('../components/conversation-message.tsx', import.meta.url),
);
export const interactionPath = fileURLToPath(
  new URL('../components/message-interaction.tsx', import.meta.url),
);
export const sources = new Map([
  [
    conversationPath,
    readFileSync(process.env.ZOEN_A11Y_CONVERSATION_SOURCE ?? conversationPath, 'utf8'),
  ],
  [
    interactionPath,
    readFileSync(process.env.ZOEN_A11Y_INTERACTION_SOURCE ?? interactionPath, 'utf8'),
  ],
]);
export const transformed = new Set<string>();
export const nativePorts = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'react-native-gesture-handler',
  'react-native-reanimated',
  'react-native-worklets',
  'expo-haptics',
  'expo-router',
  '../i18n/index.ts',
  '../media/MediaAttachment',
  '../miniapps/message-card',
  '../profile/profile-avatar',
  '../theme',
  './primitives',
  './service-message',
  './message-markdown',
]);
export async function compile(platform: 'ios' | 'web') {
  const output = await build({
    stdin: {
      contents:
        "export { ConversationMessage } from './conversation-message'; export { MessageInteraction } from './message-interaction';",
      resolveDir: dirname(conversationPath),
      loader: 'ts',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'message-accessibility-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => {
            if (path.startsWith('react-native-web/dist/exports/'))
              return { path: `web-native-port:${path.split('/').at(-1)}`, external: true };
            return nativePorts.has(path) ? { path, external: true } : undefined;
          });
          builder.onLoad(
            {
              filter:
                /(?:conversation-message(?:-face|-details)?\.tsx|use-conversation-message\.ts|message-interaction\.tsx)$/,
            },
            ({ path }) => {
              const source = sources.get(path) ?? readFileSync(path, 'utf8');
              assert.ok(source, `Unexpected component source: ${path}`);
              transformed.add(path);
              const compiled = transformSync(source, {
                filename: path,
                configFile: false,
                babelrc: false,
                presets: [require.resolve('babel-preset-expo')],
                caller: {
                  name: 'metro',
                  platform,
                  engine: 'hermes',
                  isDev: false,
                  supportsReactCompiler: true,
                },
              }).code;
              if (sources.has(path))
                assert.match(
                  compiled,
                  /react\/compiler-runtime/,
                  'Execute the actual compiler output',
                );
              return {
                contents: compiled,
                resolveDir: dirname(path),
                loader: 'js',
              };
            },
          );
        },
      },
    ],
  });
  assert.equal(
    transformed.size,
    messageSourceModules.length,
    'Every extracted production module must pass through Expo Babel',
  );
  return output.outputFiles[0]!.text;
}
export const outputs = { ios: await compile('ios'), web: await compile('web') };

export type Element = { type: unknown; props: Record<string, unknown> };
export type Effect = () => void | (() => void);
export type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
export type Hooks = {
  slots: Slot[];
  caches: unknown[][];
  cursor: number;
  cacheCursor: number;
  layout: (() => void)[];
  effects: (() => void)[];
};
export const hooks = (): Hooks => ({
  slots: [],
  caches: [],
  cursor: 0,
  cacheCursor: 0,
  layout: [],
  effects: [],
});
export function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
export const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
};
export const drain = () => new Promise<void>((done) => setImmediate(done));
export function message(patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: 'accessible-message',
    seq: '1',
    author: {
      id: 'synthetic-author',
      name: 'Nina Author',
      is_me: false,
      kind: 'Person',
    } as TimelineEntry['author'],
    at_ms: '1792320000000',
    kind: { Message: { text: 'Canonical message body 🌤️', card: null } },
    delivery: 'Sent',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
    ...patch,
  };
}
export const quote = (id: string, text = 'Canonical quoted body') => ({
  id,
  text,
  author: { id: `author-${id}`, name: `Quoted ${id}` } as TimelineEntry['author'],
});
export function translated(key: string, params: Record<string, unknown> = {}) {
  if (key === 'chat.messageActions') return `Message actions for ${params.name}`;
  if (key === 'chat.replyingTo') return `Replying to ${params.name}`;
  if (key === 'chat.editedAt') return `Edited at ${params.time}`;
  const catalog: Record<string, string> = {
    'chat.forwarded': 'Forwarded',
    'chat.reply': 'Reply',
    'chat.openReplies': 'Open replies',
    'chat.goToQuotedMessage': 'Go to quoted message',
    'chat.receiptRead': 'Read by everyone',
    'chat.receiptDelivered': 'Delivered',
    'chat.receiptSent': 'Sent',
    'presentation.deletedMessage': 'Message deleted',
    'chat.touchAndHoldToReplyOrSeeActions': 'Touch and hold for message actions',
  };
  return catalog[key] ?? `${key} ${JSON.stringify(params)}`;
}

/** Native ports are mocked; both real component functions, compiler caches,
 * effects and refs execute and persist through each explicit render/commit. */
export function fixture(initial = message(), disabled = false, platform: 'ios' | 'web' = 'ios') {
  const initialQuery = deferred<boolean>();
  const listeners = new Set<(enabled: boolean) => void>();
  const registered: ((enabled: boolean) => void)[] = [];
  const conversationHooks = hooks();
  const interactionHooks = hooks();
  let active = conversationHooks;
  let entry = initial;
  let isDisabled = disabled;
  let queries = 0;
  let removed = 0;
  let stateWrites = 0;
  const lifecycle: string[] = [];
  const calls: { action: string; entry?: TimelineEntry; target?: string; mode?: unknown }[] = [];
  const slot = () => (active.slots[active.cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    Boolean(a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i])));
  const effect = (callback: Effect, deps?: readonly unknown[], layout = false) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    (layout ? active.layout : active.effects).push(() => {
      current.cleanup?.();
      current.cleanup = callback() ?? undefined;
    });
  };
  const memo = (calculate: () => unknown, deps: readonly unknown[]) => {
    const current = slot();
    if (!same(current.deps, deps)) {
      current.deps = deps;
      current.value = calculate();
    }
    return current.value;
  };
  const ref = (value: unknown) => {
    const current = slot();
    current.value ??= { current: value };
    return current.value as { current: unknown };
  };
  const element = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const theme = resolveSkyTheme('day', new Date('2026-10-18T12:00:00'));
  const modules: Record<string, unknown> = {
    react: {
      memo: (component: unknown) => component,
      useRef: ref,
      useState: (value: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = value;
        return [
          current.value,
          (next: unknown) => {
            ++stateWrites;
            current.value = typeof next === 'function' ? next(current.value) : next;
          },
        ];
      },
      useMemo: memo,
      useCallback: (callback: unknown, deps: readonly unknown[]) => memo(() => callback, deps),
      useEffect: (callback: Effect, deps?: readonly unknown[]) => effect(callback, deps),
      useLayoutEffect: (callback: Effect, deps?: readonly unknown[]) =>
        effect(callback, deps, true),
      createContext: (value: unknown) => ({ value }),
      use: (context: { value: unknown }) => context.value,
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (active.caches[active.cacheCursor++] ??= Array(size).fill(
          Symbol.for('react.memo_cache_sentinel'),
        )),
    },
    'react/jsx-runtime': { jsx: element, jsxs: element },
    'expo-router': { useFocusEffect: (callback: Effect) => effect(callback, [callback], true) },
    'react-native': {
      AppState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
      Text: 'Text',
      View: 'View',
      Pressable: 'NativePressable',
      useWindowDimensions: () => ({ width: 402, height: 874 }),
      AccessibilityInfo: {
        isScreenReaderEnabled: () => {
          lifecycle.push('query');
          ++queries;
          return initialQuery.promise;
        },
        addEventListener: (name: string, callback: (enabled: boolean) => void) => {
          assert.equal(name, 'screenReaderChanged');
          lifecycle.push('subscribe');
          listeners.add(callback);
          registered.push(callback);
          return {
            remove: () => {
              ++removed;
              listeners.delete(callback);
            },
          };
        },
      },
    },
    'react-native-gesture-handler': {
      GestureDetector: 'GestureDetector',
      GestureStateManager: { fail() {}, activate() {} },
      usePanGesture: (config: unknown) => {
        const pan = ref({ config });
        (pan.current as { config: unknown }).config = config;
        return pan.current;
      },
    },
    'react-native-reanimated': {
      default: { View: 'AnimatedView' },
      useSharedValue: (initialValue: unknown) => {
        const value = ref(initialValue);
        return memo(
          () => ({ get: () => value.current, set: (next: unknown) => (value.current = next) }),
          [value],
        );
      },
      useAnimatedStyle: (calculate: () => unknown) => calculate(),
      useDerivedValue: (read: () => unknown) => ({ get: read }),
      Easing: { bezier: (...values: unknown[]) => values, linear: 'linear' },
      withSequence: (...values: unknown[]) => values.at(-1),
      withTiming: (value: unknown) => value,
    },
    'react-native-worklets': {
      scheduleOnRN: (callback: (...args: unknown[]) => void, ...args: unknown[]) =>
        callback(...args),
    },
    'expo-haptics': { impactAsync: async () => {}, ImpactFeedbackStyle: { Heavy: 'heavy' } },
    '../i18n/index.ts': { t: translated, locale: 'en', formatLocale: 'en-US' },
    '../theme': { useTheme: () => ({ ...theme, reducedMotion: false }) },
    '../media/MediaAttachment': { MessageMedia: 'MessageMedia' },
    '../miniapps/message-card': { MiniappMessageCard: 'MiniappMessageCard' },
    '../profile/profile-avatar': { ProfileAvatar: 'ProfileAvatar' },
    './service-message': { ServiceMessage: 'ServiceMessage' },
    './message-markdown': { MessageMarkdown: 'MessageMarkdown' },
    './primitives': { Icon: 'Icon' },
  };
  const module = {
    exports: {} as {
      ConversationMessage(props: Record<string, unknown>): Element;
      MessageInteraction(props: Record<string, unknown>): Element;
    },
  };
  runInNewContext(outputs[platform], {
    module,
    exports: module.exports,
    global: { Error },
    process: { env: { EXPO_OS: platform } },
    require: (name: string) => {
      if (name.startsWith('web-native-port:')) {
        const port = name.slice('web-native-port:'.length);
        const native = modules['react-native'] as Record<string, unknown>;
        assert.ok(Object.hasOwn(native, port), `Unexpected Web native port: ${port}`);
        return { __esModule: true, default: native[port] };
      }
      assert.ok(Object.hasOwn(modules, name), `Unexpected native accessibility port: ${name}`);
      return modules[name];
    },
  });
  function commit(instance: Hooks, render: () => Element) {
    active = instance;
    active.cursor = 0;
    active.cacheCursor = 0;
    active.layout = [];
    active.effects = [];
    const tree = render();
    for (const run of active.layout) run();
    for (const run of active.effects) run();
    return tree;
  }
  const callbacks = {
    onReply: (value: TimelineEntry) => calls.push({ action: 'reply', entry: value }),
    onThread: (value: TimelineEntry) => calls.push({ action: 'thread', entry: value }),
    onAuthor: (value: TimelineEntry) => calls.push({ action: 'author', entry: value }),
    onMenu: (value: TimelineEntry, mode: unknown) =>
      calls.push({ action: 'menu', entry: value, mode }),
    onReact: () => assert.fail('Accessibility must use canonical message actions'),
    onQuote: (target: string) => calls.push({ action: 'quote', target }),
  };
  function render() {
    const tree = commit(conversationHooks, () =>
      module.exports.ConversationMessage({
        spaceId: 'synthetic-accessibility-space',
        entry,
        groupChat: true,
        disabled: isDisabled,
        ...callbacks,
      }),
    );
    const interaction = elements(tree).find(
      (node) => node.type === module.exports.MessageInteraction,
    );
    assert.ok(interaction, 'The actual row must supply its actual interaction props');
    const wrapper = commit(interactionHooks, () =>
      module.exports.MessageInteraction(interaction.props),
    );
    const group = elements(wrapper).find((node) => node.props.collapsable === false);
    assert.ok(group, 'The real wrapper must render its native accessibility host');
    const bubble = elements(tree).find(
      (node) =>
        node.props.testID === `message-bubble-${entry.id}` ||
        node.props.testID === `message-app-card-${entry.id}`,
    );
    assert.ok(bubble);
    return { tree, wrapper, group, bubble, interaction };
  }
  return {
    render,
    calls,
    initialQuery,
    writes: () => stateWrites,
    listeners: () => listeners.size,
    registrations: () => registered.length,
    removals: () => removed,
    queries: () => queries,
    lifecycle: () => lifecycle,
    emit: (enabled: boolean) => {
      for (const callback of listeners) callback(enabled);
    },
    lateEvent: (enabled: boolean) => registered[0]?.(enabled),
    recycle: (next: TimelineEntry, disabled = false) => {
      entry = next;
      isDisabled = disabled;
      return render();
    },
    enable: async () => {
      render();
      initialQuery.resolve(true);
      await drain();
      return render();
    },
    unmount: () => {
      for (const instance of [interactionHooks, conversationHooks])
        for (const current of instance.slots) current.cleanup?.();
    },
  };
}
export const actions = (group: Element) =>
  group.props.accessibilityActions as { name: string; label: string }[] | undefined;
export function act(group: Element, name: string) {
  (group.props.onAccessibilityAction as (event: unknown) => void)({
    nativeEvent: { actionName: name },
  });
}
