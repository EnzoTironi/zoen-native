import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type { MessageInteractionProps } from '../components/message-interaction.types';
import { resolveSkyTheme } from '../theme/sky.ts';
import { messageMayContainMarkdown } from './message-markdown.ts';
import * as presentation from './message-presentation.ts';
import { transformMessageModules } from './message-source-compiler.ts';
import { messageText } from './presentation.ts';
import type { TimelineEntry } from './types';

export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
export const path = fileURLToPath(
  new URL('../components/message-interaction.tsx', import.meta.url),
);
export const compiled = transformSync(
  readFileSync(process.env.ZOEN_MESSAGE_SWIPE_SOURCE ?? path, 'utf8'),
  {
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
  },
).code;
assert.match(compiled, /react\/compiler-runtime/, 'Use the actual installed iOS compiler output');
export const ports = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'expo-router',
  'react-native-gesture-handler',
  'react-native-reanimated',
  'react-native-worklets',
  'expo-haptics',
  '../theme',
  '../i18n/index.ts',
  './primitives',
]);
export const output = await build({
  stdin: { contents: compiled, resolveDir: dirname(path), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'message-swipe-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
export type Element = { type: unknown; props: Record<string, unknown> };
export type Effect = () => void | (() => void);
export type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
export type Hooks = {
  cursor: number;
  cacheCursor: number;
  slots: Slot[];
  caches: unknown[][];
  pending: (() => void)[];
};
export const hooks = (): Hooks => ({
  cursor: 0,
  cacheCursor: 0,
  slots: [],
  caches: [],
  pending: [],
});
export function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
export function styles(node: Element): Record<string, unknown> {
  const flatten = (value: unknown): Record<string, unknown> =>
    Array.isArray(value)
      ? Object.assign({}, ...value.map(flatten))
      : ((value ?? {}) as Record<string, unknown>);
  return flatten(node.props.style);
}
export const json = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

export const conversationPath = fileURLToPath(
  new URL('../components/conversation-message.tsx', import.meta.url),
);
export const conversationCode = (
  await build({
    stdin: {
      contents: transformSync(
        readFileSync(process.env.ZOEN_MESSAGE_ALBUM_SOURCE ?? conversationPath, 'utf8'),
        {
          filename: conversationPath,
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
        },
      ).code,
      loader: 'js',
      resolveDir: dirname(conversationPath),
    },
    bundle: true,
    platform: 'node',
    plugins: [
      {
        name: 'actual-conversation-face',
        setup(builder) {
          transformMessageModules(builder, 'ios');
          builder.onResolve({ filter: /./ }, ({ path }) =>
            /^(?:\.\/conversation-message-(?:face|details)|\.\/use-conversation-message)$/.test(
              path,
            )
              ? undefined
              : { path, external: true },
          );
        },
      },
    ],
    format: 'cjs',
    write: false,
  })
).outputFiles[0]!.text;

/** Take the actual rendered metadata leaf and wrapper policy, not a hand-written album substitute. */
export function albumMetadata(delivery: TimelineEntry['delivery']) {
  const entry = {
    id: 'signed-album-first-message',
    author: { id: 'me', name: 'Me', is_me: true },
    at_ms: '1000',
    kind: {
      Message: {
        text: 'First caption',
        card: { kind_id: 'file', item_id: 'signed-file', version: 1 },
      },
    },
    delivery,
    reactions: [],
  } as unknown as TimelineEntry;
  const menus: unknown[] = [];
  const theme = resolveSkyTheme('night', new Date('2026-10-21T22:00:00'));
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const modules: Record<string, unknown> = {
    react: {
      memo: (component: unknown) => component,
      useRef: (value: unknown) => ({ current: value }),
      useState: (value: unknown) => [value, () => {}],
      useLayoutEffect: (effect: Effect) => effect(),
      useCallback: (callback: unknown) => callback,
    },
    'react/compiler-runtime': {
      c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      View: 'View',
      Text: 'Text',
      useWindowDimensions: () => ({ width: 402, height: 874, scale: 3, fontScale: 1 }),
    },
    '../core/message-presentation': presentation,
    '../core/presentation': { messageText },
    '../core/message-markdown': { messageMayContainMarkdown },
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en-US' },
    '../theme': { useTheme: () => theme },
    '../media/MediaAttachment': { MessageMedia: 'Media' },
    './message-interaction': {
      MessageInteraction: 'Interaction',
      MessagePressable: 'MessagePressable',
    },
    './message-context-source': {
      MessageContextSource: 'ContextSource',
      MessageContextControl: 'ContextControl',
    },
    './conversation-reveal-content-view': { useConversationRevealAnchor: () => null },
    './message-receipt': { MessageReceipt: 'Receipt', messageDeliveryLabel: () => delivery },
    './primitives': { Icon: 'Icon' },
  };
  const module = { exports: {} as { ConversationMessage(props: unknown): Element } };
  runInNewContext(conversationCode, {
    module,
    exports: module.exports,
    require: (name: string) => modules[name] ?? {},
  });
  const tree = module.exports.ConversationMessage({
    spaceId: 'signed-space',
    entry,
    albumContent: { width: 294, media: 'OriginalMedia', captions: 'OriginalCaptions', delivery },
    onReply() {},
    onThread() {},
    onMenu: (value: TimelineEntry, mode: string) => menus.push({ id: value.id, mode }),
  });
  const interaction = elements(tree).find((node) => node.type === 'Interaction')!;
  const metadata = elements(tree).find(
    (node) => node.props.testID === `message-metadata-${entry.id}`,
  )!;
  const action = elements(metadata).find((node) => node.type === 'MessagePressable')!;
  assert.ok(interaction && action, 'Actual album must retain its metadata action');
  return { interaction: interaction.props as MessageInteractionProps, action: action.props, menus };
}

/** Real Babel component/hooks execute. Native gesture, focus, animation and RN scheduling are explicit ports, not a device proof. */
export function fixture(initial: Partial<MessageInteractionProps> = {}) {
  const main = hooks(),
    child = hooks(),
    contextValues = new Map<unknown, unknown>();
  let current = main,
    reduced = false,
    voiceOver = false,
    focused = true;
  let props: MessageInteractionProps = {
    messageId: 'm1',
    mine: false,
    children: 'Bubble',
    onReply: () => calls.push('reply'),
    onThread: () => calls.push('thread'),
    ...initial,
  };
  let pan!: Record<string, (event: Record<string, unknown>) => void>;
  let tree!: Element, focus: { callback: Effect; cleanup?: () => void } | undefined;
  const pending: (() => void)[] = [],
    calls: string[] = [],
    impacts: string[] = [],
    states: string[] = [];
  const animation: { value: unknown; options: unknown }[] = [];
  const appListeners = new Set<(state: string) => void>(),
    a11yListeners = new Set<(enabled: boolean) => void>();
  const slot = () => (current.slots[current.cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    Boolean(
      a && b && a.length === b.length && a.every((value, index) => Object.is(value, b[index])),
    );
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const item = slot();
    if (!same(item.deps, deps)) {
      item.value = read();
      item.deps = deps;
    }
    return item.value;
  };
  const ref = (value: unknown) => {
    const item = slot();
    return (item.value ??= { current: value }) as { current: unknown };
  };
  const effect = (read: Effect, deps?: readonly unknown[]) => {
    const item = slot();
    if (same(item.deps, deps)) return;
    item.deps = deps;
    current.pending.push(() => {
      item.cleanup?.();
      item.cleanup = read() ?? undefined;
    });
  };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const modules: Record<string, unknown> = {
    react: {
      useRef: ref,
      useMemo: memo,
      useCallback: (read: unknown, deps: readonly unknown[]) => memo(() => read, deps),
      useEffect: effect,
      useLayoutEffect: effect,
      useState: (initial: unknown) => {
        const item = slot();
        if (!Object.hasOwn(item, 'value')) item.value = initial;
        return [
          item.value,
          (value: unknown) => {
            item.value = typeof value === 'function' ? value(item.value) : value;
          },
        ];
      },
      createContext: (value: unknown) => {
        const context = {};
        contextValues.set(context, value);
        return context;
      },
      use: (context: unknown) => contextValues.get(context),
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (current.caches[current.cacheCursor++] ??= Array(size).fill(
          Symbol.for('react.memo_cache_sentinel'),
        )),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'expo-router': {
      useFocusEffect: (read: Effect) =>
        effect(() => {
          focus = { callback: read };
          if (focused) focus.cleanup = read() ?? undefined;
          return () => {
            focus?.cleanup?.();
            focus = undefined;
          };
        }, [read]),
    },
    'react-native': {
      View: 'View',
      Pressable: 'Pressable',
      useWindowDimensions: () => ({ width: 402, height: 874, scale: 3, fontScale: 1 }),
      AppState: {
        currentState: 'active',
        addEventListener: (_name: string, read: (state: string) => void) => {
          appListeners.add(read);
          return { remove: () => appListeners.delete(read) };
        },
      },
      AccessibilityInfo: {
        isScreenReaderEnabled: async () => voiceOver,
        addEventListener: (_name: string, read: (enabled: boolean) => void) => {
          a11yListeners.add(read);
          return { remove: () => a11yListeners.delete(read) };
        },
      },
    },
    'react-native-gesture-handler': {
      GestureDetector: 'Detector',
      usePanGesture: (value: typeof pan) => {
        pan = value;
        return value;
      },
      GestureStateManager: {
        fail: () => states.push('fail'),
        activate: () => states.push('activate'),
      },
    },
    'react-native-reanimated': {
      default: { View: 'AnimatedView' },
      Easing: { linear: 'linear', bezier: (...values: number[]) => values },
      useSharedValue: (value: unknown) => {
        const item = ref(value);
        return memo(
          () => ({
            get: () => item.current,
            set: (next: unknown) => {
              item.current = next;
            },
          }),
          [item],
        );
      },
      useDerivedValue: (read: () => unknown) => ({ get: read }),
      useAnimatedStyle: (read: () => unknown) => read(),
      withTiming: (value: unknown, options: unknown) => {
        animation.push({ value, options });
        return value;
      },
      withSequence: (...values: unknown[]) => values.at(-1),
    },
    'react-native-worklets': {
      scheduleOnRN: (read: (...args: unknown[]) => void, ...args: unknown[]) =>
        pending.push(() => read(...args)),
    },
    'expo-haptics': {
      impactAsync: async (style: string) => {
        impacts.push(style);
      },
      ImpactFeedbackStyle: { Heavy: 'heavy' },
    },
    '../theme': {
      useTheme: () => ({ reducedMotion: reduced, colors: { muted: '#999', text: '#000' } }),
    },
    '../i18n/index.ts': { t: (key: string) => key },
    './primitives': { Icon: 'Icon' },
  };
  const module = {
    exports: {} as {
      MessageInteraction(props: MessageInteractionProps): Element;
      MessagePressable(props: Record<string, unknown>): Element;
      useMessagePressConsumption(): () => void;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    global: { Error },
    process: { env: { EXPO_OS: 'ios' } },
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected port ${name}`);
      return modules[name];
    },
  });
  function commit(instance: Hooks, render: () => Element) {
    current = instance;
    instance.cursor = 0;
    instance.cacheCursor = 0;
    instance.pending = [];
    const value = render();
    for (const apply of instance.pending) apply();
    return value;
  }
  function render(patch: Partial<MessageInteractionProps> = {}) {
    props = { ...props, ...patch };
    tree = commit(main, () => module.exports.MessageInteraction(props));
    for (const node of elements(tree))
      if (contextValues.has(node.type)) contextValues.set(node.type, node.props.value);
    return tree;
  }
  render();
  const touch = (x: number, y: number, numberOfTouches = 1) => ({
    numberOfTouches,
    handlerTag: 9,
    allTouches: [{ absoluteX: x, absoluteY: y }],
  });
  return {
    render,
    calls,
    impacts,
    animation,
    states,
    pending,
    get tree() {
      return tree;
    },
    get pan() {
      return pan;
    },
    begin(x = 90) {
      if ((pan as Record<string, unknown>).enabled === false) return;
      pan.onTouchesDown!(touch(x, 100));
    },
    move(dx: number, dy = 0, x = 90, count = 1) {
      pan.onTouchesMove!(touch(x + dx, 100 + dy, count));
    },
    update(translationX: number) {
      pan.onUpdate!({ translationX });
    },
    end(translationX: number, canceled = false, velocityX = 0) {
      pan.onDeactivate!({ translationX, canceled, velocityX });
      pan.onFinalize!({});
    },
    flush() {
      while (pending.length) pending.shift()!();
    },
    blur() {
      focused = false;
      focus?.cleanup?.();
      if (focus) focus.cleanup = undefined;
    },
    focus() {
      focused = true;
      if (focus) focus.cleanup = focus.callback() ?? undefined;
    },
    app(state: string) {
      for (const read of appListeners) read(state);
    },
    unmount() {
      for (const item of main.slots) item.cleanup?.();
    },
    reduced(value: boolean) {
      reduced = value;
      render();
    },
    voiceOver(value: boolean) {
      voiceOver = value;
      for (const read of a11yListeners) read(value);
      render();
    },
    press(props: Record<string, unknown> = {}) {
      let pressed = 0;
      const node = commit(child, () =>
        module.exports.MessagePressable({
          ...props,
          onPress: () => {
            pressed++;
            (props.onPress as (() => void) | undefined)?.();
          },
        }),
      );
      (node.props.onPress as () => void)();
      return pressed;
    },
    consume() {
      current = child;
      child.cursor = 0;
      module.exports.useMessagePressConsumption()();
    },
    indicator(direction: 'reply' | 'thread') {
      const node = elements(tree).find((item) => item.props.direction === direction)!;
      assert.equal(typeof node.type, 'function');
      return commit(child, () => (node.type as (props: unknown) => Element)(node.props));
    },
  };
}

export function swipe(f: ReturnType<typeof fixture>, distance: number) {
  f.begin();
  f.move(Math.sign(distance) * 10);
  f.update(distance);
  f.end(distance);
}
