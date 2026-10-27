import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const sdk =
  process.env.ZOEN_NATIVE_SCROLL_SDK_SOURCE ??
  fileURLToPath(
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
        // Execute the same Expo/Hermes worklet transform as the installed SDK.
        // Dependency code is not claimed to be React Compiler transformed.
        builder.onLoad({ filter: /\/useExtraContentPadding\/index\.ts$/ }, ({ path }) => ({
          contents: transformSync(readFileSync(path, 'utf8'), {
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
          }).code,
          loader: 'js',
        }));
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
export function nativeScrollIntentFixture({
  initialExtra = 240,
  initialOffset = 376,
  viewportHeight = 700,
  contentHeight = 1300,
  offset = 0,
} = {}) {
  const extra = { value: initialExtra };
  const blankSpace = { value: 0 };
  const freeze = { value: false };
  const scroll = { value: initialOffset };
  const layout = { value: { width: 390, height: viewportHeight } };
  const size = { value: { width: 390, height: contentHeight } };
  const reactions: Reaction[] = [];
  const effects: (() => void)[] = [];
  const handlers: {
    onStart?: (event: KeyboardEvent) => void;
    onEnd?: (event: KeyboardEvent) => void;
    onInteractive?: () => void;
  }[] = [];
  const reanimated = {
    __esModule: true,
    default: { View: 'AnimatedView', ScrollView: 'NativeScrollView' },
    makeMutable: <T>(value: T): Mutable<T> => ({ value }),
    useSharedValue: <T>(value: T) => ({
      value,
      get() {
        return this.value;
      },
      set(next: T) {
        this.value = next;
      },
    }),
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
    interpolate: (value: number, input: number[], output: number[]) => {
      assert.equal(input.length, 2);
      assert.equal(output.length, 2);
      const fraction = (value - input[0]!) / (input[1]! - input[0]!);
      return output[0]! + fraction * (output[1]! - output[0]!);
    },
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
    global: { Error },
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
      blankSpace,
      freeze,
      contentOffset: { x: 0, y: initialOffset },
      offset,
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
    blankSpace,
    freeze,
    scroll,
    layout,
    size,
    tick,
    props: () => (view.props.animatedProps as Mutable<Record<string, unknown>>).value,
    interactive() {
      for (const handler of handlers) handler.onInteractive?.();
    },
    keyboard(height: number) {
      for (const handler of handlers) handler.onStart?.({ height, duration: 250 });
      for (const handler of handlers) handler.onEnd?.({ height, duration: 250 });
      tick();
    },
  };
}
