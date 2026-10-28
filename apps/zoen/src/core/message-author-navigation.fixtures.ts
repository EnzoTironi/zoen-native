import { messageInfoNavigation } from './message-info-navigation.fixtures.ts';
import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type {
  MeasureMessageAnchor,
  MessageActionsProps,
  MessageAnchor,
} from '../components/message-actions.types';
import type { ClientState } from './store';
import type { Snapshot, TimelineEntry } from './types';

export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
export const sourcePath = fileURLToPath(
  new URL('../components/use-message-actions.tsx', import.meta.url),
);
export const source = readFileSync(process.env.ZOEN_MESSAGE_AUTHOR_SOURCE ?? sourcePath, 'utf8');
// Exercise the shipped iOS compiler path as well as the actual hook. Native
// ports are replaced below; canonical interaction and membership policies are not.
export const compiled = transformSync(source, {
  filename: sourcePath,
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
  caller: {
    name: 'metro',
    platform: 'ios',
    engine: 'hermes',
    isDev: false,
    supportsReactCompiler: true,
  },
}).code;
export const compilerUsesCache = compiled.includes('react/compiler-runtime');
export const externalPorts = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'expo-router',
  'expo-clipboard',
  'react-native-keyboard-controller',
  '../core/provider',
  '../media/share-message',
  '../i18n/index.ts',
  './dialogs',
  './message-actions',
  './reaction-details',
  './message-info-navigation',
]);
export const bundled = await build({
  stdin: { contents: compiled, resolveDir: dirname(sourcePath), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'message-author-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          externalPorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

export const spaceId = 'synthetic_group';
export function author(
  id = 'sender',
  kind: 'Person' | 'Agent' = 'Person',
): TimelineEntry['author'] {
  return {
    id,
    kind,
    name: id,
    handle: id,
    initials: 'QA',
    tint_hex: '#487a64',
    glyph: null,
    bio: '',
    owner_id: null,
    owner_name: null,
    owner_tint_hex: null,
    photo_sha256: null,
    photo_ready: false,
    photo_mime: null,
    is_me: false,
    is_mine: false,
  };
}
export function message(patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: 'synthetic_message',
    seq: '1',
    author: author(),
    at_ms: '1000',
    kind: { Message: { text: 'Synthetic author-navigation fixture', card: null } },
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
export function clientState(): ClientState {
  return {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      me: author('alice'),
      account: {
        identity_id: 'alice',
        device_id: 'phone',
        unlocked: true,
        setup_pending: false,
      },
      conversations: [{ id: spaceId, members: [author('alice'), author()] }],
    } as unknown as Snapshot,
    timelines: {},
    notice: null,
    activity: {} as ClientState['activity'],
  };
}
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

export type Effect = () => void | (() => void);
export type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
export type Options = { spaceId: string; entries: readonly TimelineEntry[]; disabled: boolean };
export type Actions = {
  profile(entry: TimelineEntry): Promise<void>;
  react(entry: TimelineEntry, emoji: string | null): Promise<void>;
  open(
    entry: TimelineEntry,
    mode: 'reactions' | 'actions' | 'participants' | 'emoji',
    anchor?: MessageAnchor,
    emoji?: string,
    measureAnchor?: MeasureMessageAnchor,
  ): Promise<void>;
  close(): void;
  reply(entry: TimelineEntry): void;
  thread(entry: TimelineEntry): void;
  selectedId: string | null;
  panel: { props: MessageActionsProps & { selectedEmoji?: string } };
};
export type Route = { pathname: string; params: { id: string } };
export type Read = { op: string; space_id: string; message_id: string };

export function fixture() {
  let live = clientState();
  let rendered = live;
  let options: Options = { spaceId, entries: [message()], disabled: false };
  let read: (command: Read) => Promise<TimelineEntry> = async () => message();
  let mutate: () => Promise<unknown> = async () => undefined;
  let push: (route: Route) => void = () => {};
  let keyboardVisible = false;
  let dismiss: () => Promise<void> = async () => {
    keyboardVisible = false;
  };
  let dismisses = 0;
  const reads: Read[] = [];
  const mutations: unknown[] = [];
  const routes: Route[] = [];
  const alerts: unknown[][] = [];
  const writes: unknown[] = [];
  const listeners = new Set<() => void>();
  const slots: Slot[] = [];
  const compilerCaches: unknown[][] = [];
  let cursor = 0;
  let compilerCursor = 0;
  let compiledHookCalls = 0;
  let layoutEffects: (() => void)[] = [];
  let effects: (() => void)[] = [];
  let focused = true;
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener: (event: 'focus' | 'blur', listener: () => void) => {
      focusListeners[event].add(listener);
      return () => {
        focusListeners[event].delete(listener);
      };
    },
  };
  const depsEqual = (left: readonly unknown[] | undefined, right: readonly unknown[] | undefined) =>
    Boolean(
      left &&
      right &&
      left.length === right.length &&
      left.every((value, i) => Object.is(value, right[i])),
    );
  const slot = () => (slots[cursor++] ??= {});
  const effect = (callback: Effect, deps: readonly unknown[] | undefined, layout = false) => {
    const current = slot();
    if (depsEqual(current.deps, deps)) return;
    current.deps = deps;
    (layout ? layoutEffects : effects).push(() => {
      current.cleanup?.();
      current.cleanup = callback() ?? undefined;
    });
  };
  const store = {
    getSnapshot: () => live,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    driver: {
      request: (command: Read) => {
        reads.push({ ...command });
        return read(command);
      },
    },
    request: (command: unknown) => {
      mutations.push(command);
      return mutate();
    },
  };
  const modules: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => {
        const current = slot();
        current.value ??= { current: value };
        return current.value;
      },
      useState: (value: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = value;
        return [
          current.value,
          (next: unknown) => {
            current.value = typeof next === 'function' ? next(current.value) : next;
            writes.push(current.value);
          },
        ];
      },
      useCallback: (callback: unknown, deps: readonly unknown[]) => {
        const current = slot();
        if (!depsEqual(current.deps, deps)) {
          current.deps = deps;
          current.value = callback;
        }
        return current.value;
      },
      useEffect: (callback: Effect, deps: readonly unknown[] | undefined) => effect(callback, deps),
      useLayoutEffect: (callback: Effect, deps: readonly unknown[] | undefined) =>
        effect(callback, deps, true),
    },
    'react/compiler-runtime': {
      c: (size: number) => {
        ++compiledHookCalls;
        return (compilerCaches[compilerCursor++] ??= Array(size).fill(
          Symbol.for('react.memo_cache_sentinel'),
        ));
      },
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: unknown) => ({ type, props }),
      jsxs: (type: unknown, props: unknown) => ({ type, props }),
    },
    'expo-router': {
      router: {
        push: (route: Route) => {
          push(route);
          routes.push({ pathname: route.pathname, params: { id: route.params.id } });
        },
      },
      useFocusEffect: undefined,
    },
    'expo-clipboard': {
      setStringAsync: () => assert.fail('Profile navigation must not copy content'),
    },
    'react-native-keyboard-controller': {
      KeyboardController: {
        dismiss: () => {
          ++dismisses;
          return dismiss();
        },
        isVisible: () => keyboardVisible,
      },
    },
    '../core/provider': { useClient: () => ({ ...rendered, store }) },
    '../media/share-message': {
      shareMessageMedia: () => assert.fail('Profile navigation must not export media'),
    },
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en' },
    './dialogs': {
      useDialogs: () => ({
        confirm: () => assert.fail('Profile navigation must not ask for mutation confirmation'),
        inform: (...args: unknown[]) => {
          alerts.push(args);
          return Promise.resolve();
        },
      }),
    },
    './message-actions': { MessageActions: () => null },
    './reaction-details': { ReactionDetails: () => null },
  };
  modules['./message-info-navigation'] = messageInfoNavigation(
    (modules['expo-router'] as { router: unknown }).router,
  );
  // Execute the installed Expo hook: each call owns its own React effect and
  // navigation listeners. A single fixture callback silently overwrote earlier hooks.
  const focusModule = { exports: {} as { useFocusEffect: (callback: Effect) => void } };
  runInNewContext(
    readFileSync(
      new URL('../../node_modules/expo-router/build/useFocusEffect.js', import.meta.url),
      'utf8',
    ),
    {
      exports: focusModule.exports,
      require: (name: string) => {
        if (name === 'react') return modules.react;
        if (name === './useNavigation') return { useNavigation: () => navigation };
        if (name === './link/useLoadedNavigation')
          return { useOptionalNavigation: () => navigation };
        throw new Error(`Unexpected Expo focus import: ${name}`);
      },
    },
  );
  (modules['expo-router'] as { useFocusEffect: unknown }).useFocusEffect =
    focusModule.exports.useFocusEffect;
  const module = {
    exports: {} as {
      useMessageActions(options: Options & { onReply(): void; onEdit(): void }): Actions;
    },
  };
  runInNewContext(bundled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    // Driver and router errors belong to the same JS realm as the hook in-app.
    Error,
    require: (name: string) => {
      assert.ok(
        Object.hasOwn(modules, name),
        `Unexpected native import in author-navigation test: ${name}`,
      );
      return modules[name];
    },
  });
  const render = (patch: Partial<Options> = {}) => {
    options = { ...options, ...patch };
    rendered = live;
    cursor = 0;
    compilerCursor = 0;
    layoutEffects = [];
    effects = [];
    const result = module.exports.useMessageActions({ ...options, onReply() {}, onEdit() {} });
    // Some hooks legitimately bail out of compiler memoization. If a later
    // transform emits caches, keep them persistent rather than masking them.
    if (compilerUsesCache) assert.ok(compiledHookCalls > 0);
    for (const commit of layoutEffects) commit();
    for (const commit of effects) commit();
    return result;
  };
  return {
    actions: render(),
    reads,
    mutations,
    routes,
    alerts,
    writes,
    render,
    state: () => live,
    update: (change: (state: ClientState) => ClientState) => {
      live = change(live);
      for (const listener of listeners) listener();
    },
    read: (next: typeof read) => {
      read = next;
    },
    mutate: (next: typeof mutate) => {
      mutate = next;
    },
    push: (next: typeof push) => {
      push = next;
    },
    dismiss: (next: typeof dismiss) => {
      dismiss = next;
    },
    dismisses: () => dismisses,
    keyboard: (visible: boolean) => {
      keyboardVisible = visible;
    },
    blur: () => {
      focused = false;
      for (const listener of focusListeners.blur) listener();
    },
    focus: () => {
      focused = true;
      for (const listener of focusListeners.focus) listener();
    },
    unmount: () => {
      for (const current of slots) current.cleanup?.();
    },
  };
}

export type Change = (state: ClientState) => ClientState;
export const account =
  (patch: Partial<NonNullable<Snapshot['account']>>): Change =>
  (state) => ({
    ...state,
    snapshot: { ...state.snapshot!, account: { ...state.snapshot!.account!, ...patch } },
  });
export const invalidSessions: [string, Change][] = [
  ['other owner', account({ identity_id: 'bob' })],
  ['other device', account({ device_id: 'other_phone' })],
  ['other mode', (state) => ({ ...state, mode: 'demo' })],
  ['opening', (state) => ({ ...state, phase: 'opening' })],
  ['erasing', (state) => ({ ...state, phase: 'erasing' })],
  ['onboarding', (state) => ({ ...state, phase: 'onboarding' })],
  ['failed', (state) => ({ ...state, phase: 'failed' })],
  ['erase pending', (state) => ({ ...state, erasePending: true })],
  ['locked account', account({ unlocked: false })],
  ['unfinished account', account({ setup_pending: true })],
  ['missing snapshot', (state) => ({ ...state, snapshot: null })],
  [
    'mismatched current identity',
    (state) => ({ ...state, snapshot: { ...state.snapshot!, me: author('bob') } }),
  ],
  [
    'removed source chat',
    (state) => ({ ...state, snapshot: { ...state.snapshot!, conversations: [] } }),
  ],
  [
    'removed membership',
    (state) => ({
      ...state,
      snapshot: {
        ...state.snapshot!,
        conversations: state.snapshot!.conversations.map((chat) => ({
          ...chat,
          members: [author()],
        })),
      },
    }),
  ],
];

export function anchorMeasurement() {
  const result = deferred<MessageAnchor | null>();
  const entered = deferred<void>();
  return {
    ...result,
    entered: entered.promise,
    measure: () => {
      entered.resolve(undefined);
      return result.promise;
    },
  };
}

export const liftedAnchor: MessageAnchor = { x: 30, y: 340, width: 240, height: 70 };
export const settledAnchor: MessageAnchor = { x: 30, y: 650, width: 240, height: 70 };
