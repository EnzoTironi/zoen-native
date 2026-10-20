import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { resolveSkyTheme } from '../theme/sky.ts';
import type { TimelineEntry } from './types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const conversationPath = fileURLToPath(
  new URL('../components/conversation-message.tsx', import.meta.url),
);
const interactionPath = fileURLToPath(
  new URL('../components/message-interaction.tsx', import.meta.url),
);
const sources = new Map([
  [
    conversationPath,
    readFileSync(process.env.ZOEN_A11Y_CONVERSATION_SOURCE ?? conversationPath, 'utf8'),
  ],
  [
    interactionPath,
    readFileSync(process.env.ZOEN_A11Y_INTERACTION_SOURCE ?? interactionPath, 'utf8'),
  ],
]);
const transformed = new Set<string>();
const nativePorts = new Set([
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
]);
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
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
        builder.onLoad(
          { filter: /(?:conversation-message|message-interaction)\.tsx$/ },
          ({ path }) => {
            const source = sources.get(path);
            assert.ok(source, `Unexpected component source: ${path}`);
            transformed.add(path);
            const compiled = transformSync(source, {
              filename: path,
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
            assert.match(compiled, /react\/compiler-runtime/, 'Execute the actual compiler output');
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
assert.equal(transformed.size, 2, 'Both production components must pass through iOS Babel');

type Element = { type: unknown; props: Record<string, unknown> };
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
type Hooks = {
  slots: Slot[];
  caches: unknown[][];
  cursor: number;
  cacheCursor: number;
  layout: (() => void)[];
  effects: (() => void)[];
};
const hooks = (): Hooks => ({
  slots: [],
  caches: [],
  cursor: 0,
  cacheCursor: 0,
  layout: [],
  effects: [],
});
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
};
const drain = () => new Promise<void>((done) => setImmediate(done));
function message(patch: Partial<TimelineEntry> = {}): TimelineEntry {
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
const quote = (id: string, text = 'Canonical quoted body') => ({
  id,
  text,
  author: { id: `author-${id}`, name: `Quoted ${id}` } as TimelineEntry['author'],
});
function translated(key: string, params: Record<string, unknown> = {}) {
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
function fixture(initial = message(), disabled = false) {
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
    './primitives': { Icon: 'Icon' },
  };
  const module = {
    exports: {} as {
      ConversationMessage(props: Record<string, unknown>): Element;
      MessageInteraction(props: Record<string, unknown>): Element;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    global: { Error },
    process: { env: { EXPO_OS: 'ios' } },
    require: (name: string) => {
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
      (node) => node.props.testID === `message-bubble-${entry.id}`,
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
const actions = (group: Element) =>
  group.props.accessibilityActions as { name: string; label: string }[] | undefined;
function act(group: Element, name: string) {
  (group.props.onAccessibilityAction as (event: unknown) => void)({
    nativeEvent: { actionName: name },
  });
}

test('VoiceOver standard activation opens the real row menu with a localized author action', async () => {
  const entry = message();
  const f = fixture(entry);
  const { group } = await f.enable();
  assert.equal(group.props.accessible, true);
  assert.equal(group.props.accessibilityRole, 'button');
  assert.equal(group.props.accessibilityHint, 'Message actions for Nina Author');
  assert.equal(
    actions(group)?.find((action) => action.name === 'actions')?.label,
    'Message actions for Nina Author',
  );
  assert.equal(typeof group.props.onAccessibilityTap, 'function');
  (group.props.onAccessibilityTap as () => void)();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0]?.action, 'menu');
  assert.equal(f.calls[0]?.mode, 'actions');
  assert.equal(f.calls[0]?.entry, entry);
  f.unmount();
});

test('custom quote, reply and thread actions use the current real row after recycling', async () => {
  const first = message({ reply_to: quote('original-target') });
  const f = fixture(first);
  let { group } = await f.enable();
  assert.ok(actions(group)?.some((action) => action.name === 'quote'));
  act(group, 'quote');
  assert.equal(f.calls[0]?.target, 'original-target');
  const next = message({ id: 'recycled-row', reply_to: quote('current-target') });
  ({ group } = f.recycle(next));
  act(group, 'quote');
  act(group, 'reply');
  act(group, 'thread');
  act(group, 'actions');
  assert.equal(f.calls[1]?.target, 'current-target');
  assert.equal(f.calls[2]?.entry, next);
  assert.equal(f.calls[3]?.entry, next);
  assert.equal(f.calls[4]?.entry, next);
  assert.equal(f.calls[4]?.mode, 'actions');
  ({ group } = f.recycle(message({ id: 'unquoted-row' })));
  assert.ok(!actions(group)?.some((action) => action.name === 'quote'));
  const count = f.calls.length;
  act(group, 'quote');
  act(group, 'unknown-native-action');
  assert.equal(f.calls.length, count);
  assert.equal(f.registrations(), 1, 'Recycling does not remount the screen-reader subscription');
  assert.equal(f.queries(), 1);
  f.unmount();
});

test('the grouped label retains actual author, body, quoted author/body, edit time and own receipt', async () => {
  const base = message();
  const mine = message({
    author: { ...base.author, name: 'Own Author', is_me: true },
    reply_to: quote('quoted-seed', 'Exact quoted text 🪴'),
    edited: true,
    forwarded: true,
    delivery: 'Read',
  });
  const f = fixture(mine);
  const { group } = await f.enable();
  const label = group.props.accessibilityLabel as string;
  for (const expected of [
    'Own Author: Canonical message body 🌤️',
    'Replying to Quoted quoted-seed: Exact quoted text 🪴',
    'Forwarded',
    `Edited at ${new Date(Number(mine.at_ms)).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })}`,
    'Read by everyone',
  ])
    assert.ok(label.includes(expected), `Missing spoken metadata: ${expected}`);
  const received = f.recycle({ ...mine, author: { ...mine.author, is_me: false } });
  assert.ok(!(received.group.props.accessibilityLabel as string).includes('Read by everyone'));
  f.unmount();
});

test('disabled and deleted rows remain readable static text with no advertised or executable actions', async () => {
  for (const deleted of [false, true]) {
    const entry = message({
      deleted,
      reply_to: quote('private-target', 'Private quoted body'),
    });
    const f = fixture(entry, !deleted);
    const { group, bubble } = await f.enable();
    assert.equal(group.props.accessible, true);
    assert.equal(group.props.accessibilityRole, 'text');
    assert.equal((group.props.accessibilityState as { disabled: boolean }).disabled, true);
    assert.equal(group.props.accessibilityHint, undefined);
    assert.equal(group.props.onAccessibilityTap, undefined);
    assert.equal(actions(group), undefined);
    assert.equal(bubble.props.accessibilityHint, undefined);
    for (const action of ['reply', 'thread', 'quote', 'actions']) act(group, action);
    (bubble.props.onAccessibilityTap as () => void)();
    assert.equal(f.calls.length, 0);
    if (deleted) {
      assert.equal(bubble.props.accessibilityRole, 'text');
      const label = group.props.accessibilityLabel as string;
      assert.ok(label.includes('Message deleted'));
      assert.ok(!label.includes('Canonical message body'));
      assert.ok(!label.includes('Private quoted body'));
    }
    f.unmount();
  }
});

test('native media and miniapp boundaries remain ungrouped and never hide independent child controls', async () => {
  const file = message({
    kind: {
      Message: {
        text: 'File caption',
        card: { item_id: 'file', kind_id: 'file', title: 'photo.jpg', version: 1 },
      },
    } as TimelineEntry['kind'],
  });
  const miniapp = message({
    kind: {
      Message: {
        text: 'Shared miniapp',
        card: {
          item_id: 'app',
          kind_id: 'app',
          title: 'Schedule',
          version: 1,
          app: { app_id: 'list' },
        },
      },
    } as TimelineEntry['kind'],
  });
  const voice = message({ voice: { item_id: 'voice' } as TimelineEntry['voice'] });
  for (const entry of [file, miniapp, voice]) {
    const f = fixture(entry);
    const { group, bubble, wrapper } = await f.enable();
    assert.equal(group.props.accessible, false);
    assert.equal(group.props.accessibilityRole, undefined);
    assert.equal(group.props.onAccessibilityTap, undefined);
    assert.equal(actions(group), undefined);
    assert.equal(group.props.accessibilityElementsHidden, undefined);
    assert.notEqual(group.props.importantForAccessibility, 'no-hide-descendants');
    assert.equal(bubble.type, 'View');
    assert.equal(bubble.props.accessible, false);
    assert.ok(elements(wrapper).some((node) => node.type === 'MessageMedia'));
    if (entry === miniapp) {
      assert.ok(elements(wrapper).some((node) => node.type === 'MiniappMessageCard'));
      assert.equal((bubble.props.style as Record<string, unknown>).width, 160);
    }
    act(group, 'actions');
    assert.equal(f.calls.length, 0);
    f.unmount();
  }
});

test('screen-reader on/off events update the mounted host and cleanup rejects late native events', async () => {
  const f = fixture();
  assert.equal(f.render().group.props.accessible, false);
  f.initialQuery.resolve(false);
  await drain();
  f.emit(true);
  assert.equal(f.render().group.props.accessible, true);
  f.emit(false);
  assert.equal(f.render().group.props.accessible, false);
  assert.equal(f.registrations(), 1);
  assert.equal(f.listeners(), 1);
  f.unmount();
  assert.equal(f.removals(), 1);
  assert.equal(f.listeners(), 0);
  const writes = f.writes();
  f.lateEvent(true);
  assert.equal(f.writes(), writes);
});

test('a late initial screen-reader query cannot overwrite a newer native on or off event', async () => {
  for (const enabled of [true, false]) {
    const f = fixture();
    f.render();
    assert.equal(f.lifecycle()[0], 'subscribe', 'Subscribe before querying initial native state');
    assert.equal(f.listeners(), 1, 'Subscribe before the asynchronous initial query settles');
    f.emit(enabled);
    assert.equal(f.render().group.props.accessible, enabled);
    f.initialQuery.resolve(!enabled);
    await drain();
    assert.equal(f.render().group.props.accessible, enabled);
    f.unmount();
  }
});

test('failed or detached initial queries cannot erase an observed state or update an unmounted row', async () => {
  const failed = fixture();
  failed.render();
  failed.emit(true);
  failed.initialQuery.reject(new Error('Synthetic AccessibilityInfo failure'));
  await drain();
  assert.equal(failed.render().group.props.accessible, true);
  failed.unmount();
  const detached = fixture();
  detached.render();
  detached.unmount();
  const writes = detached.writes();
  detached.initialQuery.resolve(true);
  await drain();
  assert.equal(detached.writes(), writes);
  assert.equal(detached.removals(), 1);
});
