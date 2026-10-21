import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';

const require = createRequire(import.meta.url);
const path = fileURLToPath(new URL('./app-keyboard-viewport.tsx', import.meta.url));
const { transformSync } = require('@babel/core');
const ports = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'expo-router',
  'react-native-safe-area-context',
  'react-native-reanimated',
  'react-native-worklets',
  '../components/keyboard-motion',
]);
const outputs = new Map<string, string>();
for (const platform of ['ios', 'android', 'web']) {
  const compiled = transformSync(
    readFileSync(process.env.ZOEN_APP_VIEWPORT_SOURCE ?? path, 'utf8'),
    {
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
    },
  ).code as string;
  assert.match(
    compiled,
    /react\/compiler-runtime/,
    'Actual installed platform Babel/React compiler output',
  );
  const output = await build({
    stdin: { contents: compiled, resolveDir: dirname(path), loader: 'js' },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'native-viewport-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) =>
            ports.has(path) || path.startsWith('react-native-web/')
              ? { path, external: true }
              : undefined,
          );
        },
      },
    ],
  });
  outputs.set(platform, output.outputFiles[0]!.text);
}
type Element = { type: any; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
type Hooks = {
  slots: Slot[];
  cursor: number;
  caches: unknown[][];
  cacheCursor: number;
  effects: (() => void)[];
};
const hooks = (): Hooks => ({ slots: [], cursor: 0, caches: [], cacheCursor: 0, effects: [] });
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
function fixture(platform = 'ios', enabled = true) {
  const parent = hooks(),
    child = hooks();
  let active = parent,
    tree: Element,
    focused = true,
    scope = 'owned-app';
  let bounds: { pageY: number; width: number; height: number } | null = {
    pageY: 0,
    width: 402,
    height: 874,
  };
  let window = { width: 402, height: 874 },
    insets = { top: 62, right: 0, bottom: 34, left: 0 };
  const queued: (() => void)[] = [],
    listeners = new Set<(state: string) => void>();
  const reactions: { prepare(): any; react(value: any): void }[] = [];
  let focus: { effect(): void | (() => void); cleanup?: () => void } | undefined;
  const shared = (value: any) => ({
    value,
    get() {
      return this.value;
    },
    set(next: any) {
      this.value = typeof next === 'function' ? next(this.value) : next;
    },
  });
  const keyboard = { height: shared(0), progress: shared(0) };
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const slot = () => (active.slots[active.cursor++] ??= {});
  const memo = (read: () => any, deps: readonly unknown[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.value = read();
      s.deps = deps;
    }
    return s.value;
  };
  const effect = (read: () => void | (() => void), deps: readonly unknown[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.deps = deps;
      active.effects.push(() => {
        s.cleanup?.();
        s.cleanup = read() ?? undefined;
      });
    }
  };
  const nativeState = {
    currentState: 'active',
    addEventListener: (_name: string, listener: (state: string) => void) => {
      listeners.add(listener);
      return { remove: () => listeners.delete(listener) };
    },
  };
  const modules: Record<string, any> = {
    react: {
      useRef: (initial: any) => {
        const s = slot();
        return (s.value ??= { current: initial });
      },
      useMemo: memo,
      useCallback: (callback: any, deps: any[]) => memo(() => callback, deps),
      useState: (initial: any) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value')) s.value = initial;
        return [
          s.value,
          (next: any) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
      useLayoutEffect: effect,
    },
    'react/compiler-runtime': {
      c(size: number) {
        const at = active.cacheCursor++;
        return (active.caches[at] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel')));
      },
    },
    'react/jsx-runtime': {
      jsx: (type: any, props: any) => ({ type, props }),
      jsxs: (type: any, props: any) => ({ type, props }),
    },
    'react-native': {
      AppState: nativeState,
      Platform: { OS: platform },
      useWindowDimensions: () => window,
    },
    'expo-router': {
      useFocusEffect: (callback: any) =>
        effect(() => {
          focus?.cleanup?.();
          focus = { effect: callback };
          if (focused) focus.cleanup = callback() ?? undefined;
          return () => {
            focus?.cleanup?.();
            focus = undefined;
          };
        }, [callback]),
    },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => insets,
      SafeAreaInsetsContext: { Provider: 'InsetsProvider' },
    },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView' },
      useSharedValue: (value: any) => {
        const s = slot();
        return (s.value ??= shared(value));
      },
      useAnimatedRef: () => {
        const s = slot();
        return (s.value ??= { current: 'native-frame' });
      },
      measure: () => bounds,
      useAnimatedReaction: (prepare: any, react: any) => {
        const s = slot();
        const value = (s.value ??= { prepare, react });
        value.prepare = prepare;
        value.react = react;
        if (!reactions.includes(value)) reactions.push(value);
      },
      useAnimatedStyle: (read: any) => read(),
    },
    'react-native-worklets': {
      scheduleOnRN: (callback: any, ...args: any[]) => queued.push(() => callback(...args)),
    },
    '../components/keyboard-motion': { useKeyboardMotion: () => keyboard },
  };
  const module = { exports: {} as { AppKeyboardViewport(props: any): Element } };
  runInNewContext(outputs.get(platform)!, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('react-native-web/')) {
        const key = name.split('/').at(-1)!;
        assert.ok(key in modules['react-native'], name);
        return { __esModule: true, default: modules['react-native'][key] };
      }
      assert.ok(name in modules, name);
      return modules[name];
    },
    global: { Error },
    console,
  });
  const render = () => {
    active = parent;
    parent.cursor = parent.cacheCursor = 0;
    parent.effects = [];
    const result = module.exports.AppKeyboardViewport({
      children: 'same-owned-editor',
      enabled,
      scope,
    });
    if (typeof result === 'string') return result;
    active = child;
    child.cursor = child.cacheCursor = 0;
    child.effects = [];
    tree = result.type(result.props);
    for (const commit of parent.effects) commit();
    for (const commit of child.effects) commit();
    return tree;
  };
  render();
  const flush = () => {
    while (queued.length) queued.shift()!();
    render();
  };
  const frame = (height: number, shouldFlush = true) => {
    keyboard.height.set(height);
    for (const reaction of reactions) reaction.react(reaction.prepare());
    if (shouldFlush) flush();
    else render();
  };
  const style = () => Object.assign({}, ...tree.props.style);
  return {
    frame,
    flush,
    render,
    source: () => tree?.props.children.props.children,
    style,
    safe: () => copy(tree.props.children.props.value),
    handlers: () => tree.props,
    queued: () => queued.length,
    listeners: () => listeners.size,
    resize(nextWindow: typeof window, nextBounds: typeof bounds, nextInsets = insets) {
      window = nextWindow;
      bounds = nextBounds;
      insets = nextInsets;
      render();
      tree.props.onLayout({ nativeEvent: { layout: nextBounds ?? { width: 0, height: 0 } } });
    },
    measure(next: typeof bounds) {
      bounds = next;
    },
    owner(next: string) {
      scope = next;
      render();
    },
    blur() {
      focused = false;
      focus?.cleanup?.();
      if (focus) focus.cleanup = undefined;
    },
    focus() {
      focused = true;
      if (focus) focus.cleanup = focus.effect() ?? undefined;
    },
    background(state: string) {
      nativeState.currentState = state;
      for (const listener of listeners) listener(state);
    },
    unmount() {
      for (const s of child.slots) s.cleanup?.();
    },
  };
}

test('actual compiled iOS layout uses the shared keyboard height and restores the same fullscreen editor', () => {
  const f = fixture();
  f.frame(0);
  assert.equal(f.style().paddingBottom, 0);
  assert.deepEqual(f.safe(), { top: 62, right: 0, bottom: 34, left: 0 });
  for (const [height, bottom] of [
    [12, 22],
    [150, 0],
    [336, 0],
    [200, 0],
    [20, 14],
    [0, 34],
  ]) {
    f.frame(height!);
    assert.equal(f.style().paddingBottom, height);
    assert.equal(f.safe().bottom, bottom);
    assert.equal(f.safe().top, 62);
    assert.equal(f.source(), 'same-owned-editor');
  }
});

test('partial native frames subtract only actual keyboard intersection, not the full keyboard height', () => {
  const f = fixture();
  f.resize({ width: 402, height: 874 }, { pageY: 62, width: 402, height: 600 });
  f.frame(200);
  assert.equal(f.style().paddingBottom, 0);
  f.frame(300);
  assert.equal(f.style().paddingBottom, 88);
  assert.equal(f.safe().bottom, 0);
});

test('keyboard-open portrait/landscape and transient zero/null frames retain positive geometry without changing owner', () => {
  const f = fixture();
  f.frame(336);
  f.resize(
    { width: 874, height: 402 },
    { pageY: 0, width: 402, height: 874 },
    { top: 0, right: 0, bottom: 21, left: 62 },
  );
  f.frame(216);
  assert.equal(f.style().paddingBottom, 336, 'old portrait bounds are not accepted as landscape');
  f.measure(null);
  f.frame(216);
  assert.equal(f.style().paddingBottom, 336);
  f.measure({ pageY: 0, width: 0, height: 0 });
  f.frame(216);
  assert.equal(f.style().paddingBottom, 336);
  f.measure({ pageY: 0, width: 874, height: 402 });
  f.frame(216);
  assert.equal(f.style().paddingBottom, 216);
  assert.deepEqual(f.safe(), { top: 0, right: 0, bottom: 0, left: 62 });
  f.frame(0);
  assert.deepEqual(f.safe(), { top: 0, right: 0, bottom: 21, left: 62 });
  assert.equal(f.source(), 'same-owned-editor');
});

test('invalid native keyboard heights do not publish invalid or collapsed viewports', () => {
  const f = fixture();
  f.frame(200);
  for (const bad of [NaN, Infinity, -2, 900]) {
    f.frame(bad);
    assert.equal(f.style().paddingBottom, 200);
  }
  f.frame(0);
  assert.equal(f.style().paddingBottom, 0);
});

for (const retire of ['blur', 'background', 'scope', 'unmount'] as const) {
  test(`queued keyboard publication cannot cross ${retire} and a later active lifetime`, () => {
    const f = fixture();
    f.frame(336, false);
    assert.ok(f.queued() > 0);
    if (retire === 'blur') {
      f.blur();
      f.focus();
    }
    if (retire === 'background') {
      f.background('inactive');
      f.background('active');
    }
    if (retire === 'scope') f.owner('different-canonical-app');
    if (retire === 'unmount') f.unmount();
    f.flush();
    assert.equal(
      f.safe().bottom,
      34,
      'old publication cannot remove the fresh lifetime home inset',
    );
    if (retire !== 'unmount') {
      f.frame(336);
      assert.equal(f.safe().bottom, 0);
    } else assert.equal(f.listeners(), 0);
  });
}

test('retired native layout callbacks only request a fresh measure and cannot publish old bounds', () => {
  const f = fixture();
  const old = f.handlers().onLayout;
  f.owner('new-app');
  f.measure({ pageY: 0, width: 402, height: 600 });
  old({ nativeEvent: { layout: { width: 402, height: 874 } } });
  f.frame(300);
  assert.equal(f.style().paddingBottom, 26);
});

test('non-iOS and inline Apps keep their existing container and do not subscribe to keyboard ownership', () => {
  for (const [platform, enabled] of [
    ['android', true],
    ['web', true],
    ['ios', false],
  ] as const) {
    const f = fixture(platform, enabled);
    assert.equal(f.render(), 'same-owned-editor');
    assert.equal(f.listeners(), 0);
  }
});
