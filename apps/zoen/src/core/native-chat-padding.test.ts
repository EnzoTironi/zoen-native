import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const sourcePath = fileURLToPath(
  new URL('../components/conversation-viewport.tsx', import.meta.url),
);
const sdkPath = fileURLToPath(
  new URL(
    '../../node_modules/react-native-keyboard-controller/src/components/KeyboardChatScrollView/useExtraContentPadding/index.ts',
    import.meta.url,
  ),
);
const compiled = transformSync(
  `${readFileSync(process.env.ZOEN_NATIVE_CHAT_PADDING_SOURCE ?? sourcePath, 'utf8')}
export { NativeConversationScrollView as TestScroller, NativeScrollContext as TestContext };`,
  {
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
  },
).code;
const ports = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'react-native-reanimated',
  'react-native-keyboard-controller',
  'react-native-safe-area-context',
]);
const output = await build({
  stdin: {
    contents: `${compiled}\nexports.sdkPadding = require(${JSON.stringify(sdkPath)}).useExtraContentPadding;`,
    resolveDir: dirname(sourcePath),
    loader: 'js',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'native-chat-padding-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (path === '../../../architecture')
            return { path: 'native-architecture', external: true };
          return ports.has(path) ? { path, external: true } : undefined;
        });
      },
    },
  ],
});

type Shared = { value: number; get(): number; set(value: number): void };
type Element = { type: unknown; props: Record<string, unknown> };
type Callback = (...args: unknown[]) => void;
type Reaction = {
  read(): unknown;
  run(value: unknown, previous: unknown): void;
  previous: unknown;
};
function shared(value: number): Shared {
  return {
    value,
    get() {
      return this.value;
    },
    set(next) {
      this.value = next;
    },
  };
}

function fixture({ deferSDKInitialization = false, observeInset = true } = {}) {
  const composer = shared(96);
  const contentOffsetY = shared(12);
  const scroll = shared(12);
  const nativeLayout = sharedValuesForSize(0, 0);
  const nativeContent = sharedValuesForSize(0, 0);
  const reactions: Reaction[] = [];
  const sharedValues: Shared[] = [];
  const states: unknown[] = [];
  const caches: unknown[][] = [];
  const forwards: [string, unknown[]][] = [];
  const ref = { current: null };
  let sharedCursor = 0;
  let stateCursor = 0;
  let compilerCursor = 0;
  let sdkInitialized = !deferSDKInitialization;
  let node!: Element;
  const modules: Record<string, unknown> = {
    react: {
      createContext: (value: unknown) => ({ value }),
      forwardRef: (render: unknown) => render,
      use: (context: { value: unknown }) => context.value,
      useCallback: (callback: unknown) => callback,
      useMemo: (calculate: () => unknown) => calculate(),
      useState: (initial: unknown) => {
        const index = stateCursor++;
        if (!(index in states)) states[index] = initial;
        return [
          states[index],
          (next: unknown) => {
            states[index] = next;
          },
        ];
      },
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (caches[compilerCursor++] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    'react/jsx-runtime': { jsx: (type: unknown, props: Element['props']) => ({ type, props }) },
    'react-native': { View: 'View', Platform: { OS: 'ios' } },
    'native-architecture': { IS_FABRIC: true },
    'react-native-keyboard-controller': { KeyboardChatScrollView: 'SDKScroller' },
    'react-native-safe-area-context': {},
    'react-native-reanimated': {
      default: {},
      useSharedValue: (initial: number) => (sharedValues[sharedCursor++] ??= shared(initial)),
      useDerivedValue: (calculate: () => number) => ({
        get value() {
          return calculate();
        },
      }),
      useAnimatedStyle: (calculate: () => unknown) => calculate(),
      useAnimatedReaction: (read: Reaction['read'], run: Reaction['run']) => {
        reactions.push({ read, run, previous: null });
      },
      scrollTo: () => assert.fail('iOS Fabric should use the SDK contentOffsetY path'),
    },
  };
  const module = {
    exports: {} as {
      TestScroller(props: Record<string, unknown>, ref: unknown): Element;
      TestContext: { value: unknown };
      sdkPadding(options: Record<string, unknown>): void;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    global: { Error },
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected native import: ${name}`);
      return modules[name];
    },
  });
  const forwarded =
    (name: string): Callback =>
    (...args) => {
      forwards.push([name, args]);
    };
  const props = {
    onLayout: forwarded('layout'),
    onContentSizeChange: forwarded('content'),
    onScroll: forwarded('scroll'),
  };
  module.exports.TestContext.value = {
    padding: composer,
    offset: 14,
    onNativeLayout: forwarded('native-layout'),
    onNativeScroll: forwarded('native-scroll'),
    onInitialInset: observeInset ? forwarded('inset') : undefined,
  };
  const render = () => {
    sharedCursor = stateCursor = compilerCursor = 0;
    node = module.exports.TestScroller(props, ref);
    return node;
  };
  const react = () => {
    if (!sdkInitialized) return;
    for (const reaction of reactions) {
      const value = reaction.read();
      if (reaction.previous === null || !Object.is(value, reaction.previous)) {
        const previous = reaction.previous;
        reaction.previous = value;
        reaction.run(value, previous);
      }
    }
  };
  render();
  // Execute the installed SDK hook: only native shared values/event registration
  // are substituted. Its delta, clamping and Fabric offset policy stay real.
  module.exports.sdkPadding({
    scrollViewRef: {},
    extraContentPadding: {
      get value() {
        return (node.props.extraContentPadding as Shared).value;
      },
    },
    keyboardPadding: shared(0),
    blankSpace: shared(0),
    scroll,
    layout: nativeLayout,
    size: nativeContent,
    contentOffsetY,
    inverted: false,
    keyboardLiftBehavior: 'always',
    freeze: { value: false },
  });
  react();
  return {
    composer,
    contentOffsetY,
    forwards,
    ref,
    render,
    react,
    padding: () => (node.props.extraContentPadding as Shared).value,
    node: () => node,
    initializeSDK: () => {
      sdkInitialized = true;
      react();
    },
    call: (name: string, ...args: unknown[]) => {
      // The real KeyboardChatScrollView updates native geometry before invoking
      // consumer callbacks; native scroll events update its offset shared value.
      if (name === 'onLayout')
        nativeLayout.value = (args[0] as ReturnType<typeof layout>).nativeEvent.layout;
      if (name === 'onContentSizeChange')
        nativeContent.value = { width: args[0] as number, height: args[1] as number };
      if (name === 'onScroll')
        scroll.set(
          (args[0] as { nativeEvent: { contentOffset: { y: number } } }).nativeEvent.contentOffset
            .y,
        );
      (node.props[name] as Callback)(...args);
      render();
      react();
    },
  };
}

function sharedValuesForSize(width: number, height: number) {
  return { value: { width, height } };
}
const layout = (height = 812, width = 390) => ({ nativeEvent: { layout: { width, height } } });

for (const order of [
  ['layout', 'content'],
  ['content', 'layout'],
]) {
  test(`compiled native chat gives the SDK a measured padding transition: ${order.join(' → ')}`, () => {
    const f = fixture();
    assert.equal(f.node().type, 'SDKScroller');
    assert.equal(f.node().props.ref, f.ref);
    assert.equal(f.padding(), 0, 'mount at zero so the actual SDK does not skip the reservation');
    assert.equal(f.contentOffsetY.value, 12);
    f.call('onContentInsetChange', { bottom: 0 });
    assert.equal(f.padding(), 0);
    assert.ok(
      !f.forwards.some(([name]) => name === 'inset'),
      'initial zero only acknowledges SDK initialization',
    );
    const events = { layout: layout(), content: [390, 824] };
    const invoke = (name: string) =>
      name === 'layout'
        ? f.call('onLayout', events.layout)
        : f.call('onContentSizeChange', ...events.content);
    invoke(order[0]!);
    assert.equal(f.padding(), 0, 'one measurement cannot authorize the initial reservation');
    invoke(order[1]!);
    assert.equal(f.padding(), 96);
    assert.equal(
      f.contentOffsetY.value,
      108,
      'the real SDK applies 12 + 96 via Fabric contentOffsetY',
    );
    f.call('onContentInsetChange', { bottom: 96 });
    assert.equal(f.forwards.find(([name]) => name === 'inset')![1][0], 96);
    assert.equal(f.forwards.find(([name]) => name === 'layout')![1][0], events.layout);
    assert.deepEqual(f.forwards.find(([name]) => name === 'content')![1], events.content);
    const scroll = { nativeEvent: { contentOffset: { y: 108 } } };
    f.call('onScroll', scroll);
    assert.equal(f.forwards.find(([name]) => name === 'scroll')![1][0], scroll);
    assert.equal(f.forwards.find(([name]) => name === 'native-scroll')![1][0], scroll);
    f.composer.set(120);
    f.react();
    assert.equal(f.padding(), 120);
    assert.equal(
      f.contentOffsetY.value,
      132,
      'later growth follows the current native offset through the real SDK',
    );
  });
}

test('zero and invalid native geometry cannot release the initial composer reservation', () => {
  const f = fixture();
  f.call('onContentInsetChange', { bottom: 0 });
  for (const value of [0, -1, NaN, Infinity]) {
    f.call('onLayout', layout(value));
    f.call('onContentSizeChange', 390, value);
    assert.equal(f.padding(), 0);
    assert.equal(f.contentOffsetY.value, 12);
  }
  for (const width of [0, -1, NaN, Infinity]) {
    f.call('onLayout', layout(812, width));
    f.call('onContentSizeChange', width, 824);
    assert.equal(f.padding(), 0);
  }
  f.call('onLayout', layout());
  assert.equal(f.padding(), 0);
  f.call('onContentSizeChange', 390, 824);
  assert.equal(f.padding(), 96);
  assert.equal(f.contentOffsetY.value, 108);
});

test('early native measurements cannot bypass the SDK initial zero reaction', () => {
  const f = fixture({ deferSDKInitialization: true });
  f.call('onLayout', layout());
  f.call('onContentSizeChange', 390, 824);
  assert.equal(f.padding(), 0, 'both geometry callbacks may precede the passive SDK mapper setup');
  assert.equal(f.contentOffsetY.value, 12);
  for (const bottom of [-1, NaN, Infinity]) {
    f.call('onContentInsetChange', { bottom });
    assert.equal(f.padding(), 0, 'invalid insets cannot acknowledge SDK initialization');
  }
  f.initializeSDK();
  assert.equal(
    f.contentOffsetY.value,
    12,
    'the actual first SDK reaction sees zero and skips only its null previous',
  );
  f.call('onContentInsetChange', { bottom: 0 });
  assert.equal(f.padding(), 96);
  assert.equal(f.contentOffsetY.value, 108, 'the later reservation is a real observed SDK delta');
});

test('SDK initialization does not require an external inset observer and repeated measurements do not scroll again', () => {
  const f = fixture({ observeInset: false });
  assert.equal(typeof f.node().props.onContentInsetChange, 'function');
  f.call('onLayout', layout());
  f.call('onContentSizeChange', 390, 824);
  assert.equal(f.padding(), 0);
  f.call('onContentInsetChange', { bottom: 0 });
  assert.equal(f.padding(), 96);
  assert.equal(f.contentOffsetY.value, 108);
  for (let i = 0; i < 2; i++) {
    f.call('onLayout', layout());
    f.call('onContentSizeChange', 390, 824);
    f.call('onContentInsetChange', { bottom: 96 });
    assert.equal(f.contentOffsetY.value, 108);
  }
  assert.ok(!f.forwards.some(([name]) => name === 'inset'));
});
