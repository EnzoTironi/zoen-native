import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const sdk = fileURLToPath(
  new URL('../../node_modules/react-native-keyboard-controller/src/components/', import.meta.url),
);
const bundled = await build({
  stdin: {
    contents: `export {default as Chat} from ${JSON.stringify(`${sdk}KeyboardChatScrollView/index.tsx`)};
      export {default as Bottom} from ${JSON.stringify(`${sdk}ScrollViewWithBottomPadding/index.tsx`)};`,
    resolveDir: sdk,
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  resolveExtensions: ['.ios.ts', '.ios.tsx', '.ts', '.tsx', '.js'],
  write: false,
  plugins: [
    {
      name: 'native-scroll-host-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (
            ['react', 'react/jsx-runtime', 'react-native', 'react-native-reanimated'].includes(path)
          )
            return { path, external: true };
          if (/^\.\.\/(?:\.\.\/)*reanimated$/.test(path))
            return { path: 'react-native-reanimated', external: true };
          if (path.endsWith('/architecture'))
            return { path: 'native-architecture', external: true };
          if (path.endsWith('/bindings')) return { path: 'native-bindings', external: true };
          if (path.endsWith('/useScrollState'))
            return { path: 'native-scroll-state', external: true };
          if (path.endsWith('/useCombinedRef'))
            return { path: 'native-combined-ref', external: true };
          if (/^\.\.\/(?:\.\.\/)*hooks$/.test(path))
            return { path: 'native-keyboard-events', external: true };
          return undefined;
        });
      },
    },
  ],
});

type Mutable<T> = { value: T };
type Node = { type: unknown; props: Record<string, unknown> };
type Reaction = {
  read(): unknown;
  run(value: unknown, previous: unknown): void;
  previous: unknown;
};
type KeyboardEvent = { height: number; duration: number };

/** Actual pinned SDK keyboard, padding correction, and animatedProps; only host ports are replaced. */
export function nativeScrollIntentFixture() {
  const extra = { value: 240 };
  const freeze = { value: false };
  const scroll = { value: 376 };
  const layout = { value: { width: 390, height: 700 } };
  const size = { value: { width: 390, height: 1300 } };
  const reactions: Reaction[] = [];
  const effects: (() => void)[] = [];
  const handlers: {
    onStart?: (event: KeyboardEvent) => void;
    onEnd?: (event: KeyboardEvent) => void;
  }[] = [];
  const reanimated = {
    __esModule: true,
    default: { View: 'AnimatedView', ScrollView: 'NativeScrollView' },
    makeMutable: <T>(value: T): Mutable<T> => ({ value }),
    useSharedValue: <T>(value: T): Mutable<T> => ({ value }),
    useAnimatedRef: () => ({}),
    useDerivedValue: <T>(read: () => T) => ({
      get value() {
        return read();
      },
    }),
    useAnimatedProps: <T>(read: () => T) => ({
      get value() {
        return read();
      },
    }),
    useAnimatedStyle: (read: () => unknown) => read(),
    useAnimatedReaction: (read: Reaction['read'], run: Reaction['run']) =>
      reactions.push({ read, run, previous: null }),
    runOnJS: (callback: unknown) => callback,
    scrollTo: () => assert.fail('iOS Fabric must retain its atomic animatedProps path'),
  };
  const ports: Record<string, unknown> = {
    react: {
      forwardRef: (render: unknown) => render,
      useEffect: (work: () => void) => effects.push(work),
      useCallback: (callback: unknown) => callback,
      useMemo: (read: () => unknown) => read(),
    },
    'react/jsx-runtime': {
      Fragment: 'Fragment',
      jsx: (type: unknown, props: Node['props']) => ({ type, props }),
      jsxs: (type: unknown, props: Node['props']) => ({ type, props }),
    },
    'react-native': {
      Platform: { OS: 'ios' },
      StyleSheet: { create: (styles: unknown) => styles },
    },
    'react-native-reanimated': reanimated,
    'native-architecture': { IS_FABRIC: true },
    'native-bindings': { ClippingScrollView: 'NativeWrapper' },
    'native-keyboard-events': {
      useKeyboardHandler: (handler: (typeof handlers)[number]) => handlers.push(handler),
    },
    'native-combined-ref': { __esModule: true, default: (ref: unknown) => ref },
    'native-scroll-state': {
      __esModule: true,
      default: () => ({
        layout,
        size,
        offset: scroll,
        onLayout: () => {},
        onContentSizeChange: () => {},
      }),
    },
  };
  const module = {
    exports: {} as {
      Chat(props: Record<string, unknown>, ref: unknown): Node;
      Bottom(props: Record<string, unknown>, ref: unknown): Node;
    },
  };
  runInNewContext(bundled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(ports, name), `Unexpected native import: ${name}`);
      return ports[name];
    },
  });
  const chat = module.exports.Chat(
    {
      extraContentPadding: extra,
      freeze,
      contentOffset: { x: 0, y: 376 },
      keyboardLiftBehavior: 'always',
      children: null,
    },
    {},
  );
  const bottom = (chat.props.children as Node[])[0]!;
  const wrapper = module.exports.Bottom(bottom.props, {});
  const view = wrapper.props.children as Node;
  for (const effect of effects) effect();
  const tick = () => {
    for (const reaction of reactions) {
      const next = reaction.read();
      if (!Object.is(next, reaction.previous)) {
        const previous = reaction.previous;
        reaction.previous = next;
        reaction.run(next, previous);
      }
    }
  };
  tick();
  return {
    extra,
    freeze,
    scroll,
    tick,
    props: () => (view.props.animatedProps as Mutable<Record<string, unknown>>).value,
    keyboard(height: number) {
      for (const handler of handlers) handler.onStart?.({ height, duration: 250 });
      for (const handler of handlers) handler.onEnd?.({ height, duration: 250 });
      tick();
    },
  };
}
