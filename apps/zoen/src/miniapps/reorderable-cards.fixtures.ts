import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import type { ItemDetail } from '../core/generated/ItemDetail';
import type { ReorderableAppCards } from './reorderable-cards';

export type Element = { type: unknown; props: Record<string, unknown> };
export type Effect = () => void | (() => void);
export type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
export type Gesture = Record<string, (...args: never[]) => unknown>;
export const exportsByPort: Record<string, string[]> = {
  react: [
    'useCallback',
    'useEffect',
    'useLayoutEffect',
    'useRef',
    'useState',
    'useSyncExternalStore',
  ],
  'react/jsx-runtime': ['jsx', 'jsxs'],
  'expo-router': ['useFocusEffect'],
  'expo-haptics': ['impactAsync', 'selectionAsync', 'ImpactFeedbackStyle'],
  'react-native': ['AccessibilityInfo', 'AppState', 'View'],
  'react-native-gesture-handler': ['GestureDetector', 'usePanGesture'],
  'react-native-reanimated': [
    'cancelAnimation',
    'Easing',
    'measure',
    'scrollTo',
    'useAnimatedRef',
    'useAnimatedScrollHandler',
    'useAnimatedStyle',
    'useFrameCallback',
    'useSharedValue',
    'withRepeat',
    'withSequence',
    'withSpring',
    'withTiming',
  ],
  'react-native-worklets': ['scheduleOnRN'],
  './compact-card': ['CompactMiniappCard'],
  './card-edit-actions': ['AppCardEditActions'],
  '../theme': ['useTheme'],
  '../i18n': ['t'],
};
export const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('./reorderable-cards.tsx', import.meta.url), 'utf8'),
    resolveDir: new URL('.', import.meta.url).pathname,
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  define: { 'process.env.EXPO_OS': '"ios"' },
  plugins: [
    {
      name: 'card-runtime-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          path in exportsByPort ? { path, namespace: 'card-port' } : undefined,
        );
        builder.onLoad({ filter: /.*/, namespace: 'card-port' }, ({ path }) => ({
          contents:
            `const p=require('card-test-ports')[${JSON.stringify(path)}];\n` +
            (path === 'react-native-reanimated' ? 'export default p.default;\n' : '') +
            exportsByPort[path]!.map((name) => `export const ${name}=p.${name};`).join('\n'),
        }));
        builder.onResolve({ filter: /^card-test-ports$/ }, ({ path }) => ({
          path,
          external: true,
        }));
      },
    },
  ],
});
export const require = createRequire(import.meta.url);
export const tick = async () => {
  for (let i = 0; i < 24; i++) await Promise.resolve();
};
export function fixture(reducedMotion = false, queuedScheduler = false) {
  const scheduled: (() => unknown)[] = [];
  let current!: {
    slots: Slot[];
    cursor: number;
    effects: (() => void)[];
    focus?: Effect;
    pan?: Gesture;
    frame?: (info: unknown) => void;
  };
  let appState: ((state: string) => void) | undefined;
  let editing = false,
    opens = 0,
    haptics = 0;
  const writes: { next: string[]; expected: string[] }[] = [];
  const announcements: string[] = [];
  const actions: string[] = [];
  const scrolls: number[] = [];
  const springs: unknown[] = [];
  let hold: Promise<unknown> | undefined;
  const shared = <T>(value: T) => ({
    get: () => value,
    set: (next: T) => {
      value = next;
    },
  });
  const slot = () => (current.slots[current.cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const at = slot();
    if (!same(at.deps, deps)) {
      at.deps = deps;
      at.value = read();
    }
    return at.value;
  };
  const effect = (read: Effect, deps?: readonly unknown[]) => {
    const at = slot();
    if (same(at.deps, deps)) return;
    at.deps = deps;
    current.effects.push(() => {
      at.cleanup?.();
      at.cleanup = read() ?? undefined;
    });
  };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports = {
    react: {
      useState: (initial: unknown) => {
        const at = slot();
        if (!Object.hasOwn(at, 'value'))
          at.value = typeof initial === 'function' ? initial() : initial;
        return [
          at.value,
          (next: unknown) => {
            at.value = typeof next === 'function' ? next(at.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const at = slot();
        at.value ??= { current: initial };
        return at.value;
      },
      useCallback: (read: unknown, deps: readonly unknown[]) => memo(() => read, deps),
      useEffect: effect,
      useLayoutEffect: effect,
      useSyncExternalStore: (_subscribe: unknown, read: () => unknown) => read(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'expo-router': {
      useFocusEffect: (read: Effect) => {
        current.focus = read;
      },
    },
    'expo-haptics': {
      impactAsync: async () => {
        haptics++;
      },
      selectionAsync: async () => {
        haptics++;
      },
      ImpactFeedbackStyle: { Light: 'light' },
    },
    'react-native': {
      View: 'View',
      AccessibilityInfo: { announceForAccessibility: (text: string) => announcements.push(text) },
      AppState: {
        currentState: 'active',
        addEventListener: (_name: string, read: (value: string) => void) => {
          appState = read;
          return { remove() {} };
        },
      },
    },
    'react-native-gesture-handler': {
      GestureDetector: 'GestureDetector',
      usePanGesture: (config: Gesture) => {
        current.pan = config;
        return config;
      },
    },
    'react-native-reanimated': {
      default: { View: 'AnimatedView', ScrollView: 'AnimatedScrollView' },
      cancelAnimation() {},
      Easing: { sin: 'sin', inOut: (v: unknown) => v },
      measure: (ref: { viewport: unknown }) => ref.viewport,
      scrollTo: (_ref: unknown, x: number) => scrolls.push(x),
      useAnimatedRef: () => memo(() => ({}), []),
      useAnimatedScrollHandler: (read: unknown) => read,
      useAnimatedStyle: (read: () => unknown) => ({ read }),
      useFrameCallback: (read: (info: unknown) => void) => {
        current.frame = read;
      },
      useSharedValue: (initial: unknown) => memo(() => shared(initial), []),
      withRepeat: (value: unknown) => value,
      withSequence: (...values: unknown[]) => values.at(-1),
      withSpring: (value: unknown) => {
        springs.push(value);
        return value;
      },
      withTiming: (value: unknown) => value,
    },
    'react-native-worklets': {
      scheduleOnRN: (read: (...args: unknown[]) => unknown, ...args: unknown[]) => {
        if (queuedScheduler) scheduled.push(() => read(...args));
        else read(...args);
      },
    },
    './compact-card': { CompactMiniappCard: 'Card' },
    './card-edit-actions': { AppCardEditActions: 'EditActions' },
    '../theme': { useTheme: () => ({ reducedMotion, colors: { text: '#ffffff' } }) },
    '../i18n': { t: (key: string) => key },
  };
  const module = { exports: {} as { ReorderableAppCards: typeof ReorderableAppCards } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => (name === 'card-test-ports' ? ports : require(name)),
    console,
  });
  function mount(component: (props: never) => unknown, initial: unknown) {
    const hooks = {
      slots: [] as Slot[],
      cursor: 0,
      effects: [] as (() => void)[],
      focus: undefined as Effect | undefined,
      pan: undefined as Gesture | undefined,
      frame: undefined as ((info: unknown) => void) | undefined,
    };
    let props = initial,
      blur: (() => void) | undefined;
    return {
      render(next = props) {
        props = next;
        current = hooks;
        hooks.cursor = 0;
        hooks.effects = [];
        const result = component(props as never) as Element;
        for (const commit of hooks.effects) commit();
        return result;
      },
      focus() {
        blur = hooks.focus?.() ?? undefined;
      },
      blur() {
        blur?.();
        blur = undefined;
      },
      unmount() {
        blur?.();
        for (const at of hooks.slots) at.cleanup?.();
      },
      gesture(name: string, event = {}) {
        hooks.pan?.[name]?.(event as never);
      },
      holdAt(x: number, y: number) {
        const config = hooks.pan as unknown as { enabled: boolean; hitSlop?: { top?: number } };
        // Native RNGHHitSlopInsetRect: origin.y -= top; height += top.
        // This port models admission, rather than merely calling onPress.
        const top = -(config.hitSlop?.top ?? 0);
        if (!config.enabled || x < 0 || x >= 160 || y < top || y >= 160) return false;
        hooks.pan?.onTouchesDown?.({} as never);
        hooks.pan?.onActivate?.({ absoluteX: x, absoluteY: y } as never);
        return true;
      },
      frame() {
        hooks.frame?.({ timeSincePreviousFrame: 16 });
      },
    };
  }
  const items = ['a', 'b', 'c'].map((id) => ({ id, title: id, app: {} })) as ItemDetail[];
  const scrollPort = {
    offset: shared(0),
    extent: shared(600),
    minimum: shared(0),
    horizontal: true,
    onScroll() {},
  };
  const props = {
    items,
    columns: 3,
    horizontal: true,
    scrollPort,
    scrollRef: { viewport: { pageX: 0, pageY: 0, width: 300, height: 300 } },
    editing,
    onEditingChange: (value: boolean) => {
      editing = value;
    },
    onReorder: async (next: readonly string[], expected: readonly string[]) => {
      writes.push({ next: [...next], expected: [...expected] });
      if (hold) await hold;
    },
    onRemove: async (item: ItemDetail) => {
      actions.push(`remove:${item.id}`);
      if (hold) await hold;
    },
    onTogglePin: async (item: ItemDetail) => {
      actions.push(`pin:${item.id}`);
      if (hold) await hold;
    },
    isPinned: () => false,
    onOpen: () => {
      opens++;
    },
  };
  const root = mount(module.exports.ReorderableAppCards as never, props);
  let tree = root.render();
  root.focus();
  const children = () => tree.props.children as Element[];
  const child = mount(children()[0]!.type as never, children()[0]!.props);
  let childTree = child.render();
  const render = () => {
    tree = root.render({ ...props, editing });
    childTree = child.render(children()[0]!.props);
  };
  const card = () => children()[0]!.props.children as Element;
  const animatedTree = () =>
    childTree.type === 'AnimatedView' ? childTree : (childTree.props.children as Element);
  const style = () => {
    const animated = animatedTree();
    return (animated.props.style as { read?: () => unknown }[]).find((v) => v.read)!.read!() as {
      transform: Record<string, unknown>[];
    };
  };
  return {
    root,
    child,
    props,
    geometry: () => ({
      container: tree.props.style as Record<string, unknown>,
      cards: children().map((element) => element.props),
      nativeFrame: animatedTree().props.style as Record<string, unknown>[],
    }),
    scrollPort,
    render,
    card,
    cards: () => children().map((child) => child.props.children as Element),
    nativeCard(id: string) {
      const node = children().find((child) => child.props.id === id)!;
      const native = mount(node.type as never, node.props);
      native.render();
      return native;
    },
    actions,
    style,
    writes,
    announcements,
    scrolls,
    springs,
    get editing() {
      return editing;
    },
    get opens() {
      return opens;
    },
    get haptics() {
      return haptics;
    },
    flush() {
      let remaining = 100;
      while (scheduled.length && remaining-- > 0) scheduled.shift()!();
      assert.ok(remaining > 0, 'bounded native-to-RN task queue');
    },
    get scheduledCount() {
      return scheduled.length;
    },
    done() {
      editing = false;
      render();
      render();
    },
    hold(promise: Promise<unknown>) {
      hold = promise;
    },
    state(value: string) {
      appState?.(value);
    },
    activate() {
      child.gesture('onTouchesDown');
      child.gesture('onActivate', { absoluteX: 100, absoluteY: 100 });
      render();
    },
    move(x: number, absoluteX = 100) {
      child.gesture('onUpdate', { translationX: x, translationY: 0, absoluteX, absoluteY: 100 });
    },
    release(canceled = false) {
      child.gesture('onDeactivate', { canceled });
      child.gesture('onFinalize');
    },
  };
}

// RNGH3's display:contents detector measures its child's untransformed layout.
// iOS shouldReceiveTouch applies negative hitSlop in the detector's coordinate
// space, so translations must be inherited from ancestors, not only its child.
export function nativeDetectorFrame(tree: Element) {
  let x = 0,
    y = 0,
    current = tree;
  while (current.type !== 'GestureDetector') {
    const styles = Array.isArray(current.props.style) ? current.props.style : [current.props.style];
    for (const value of styles) {
      if (!value) continue;
      const style = typeof value.read === 'function' ? value.read() : value;
      for (const transform of style.transform ?? []) {
        x += transform.translateX ?? 0;
        y += transform.translateY ?? 0;
      }
    }
    current = current.props.children as Element;
    assert.ok(current && !Array.isArray(current), 'one detector child in card wrapper');
  }
  const target = current.props.children as Element;
  const targetStyles = Array.isArray(target.props.style)
    ? target.props.style
    : [target.props.style];
  const targetLayout = Object.assign({}, ...targetStyles.filter(Boolean).filter((v) => !v.read));
  return { x, y, width: targetLayout.width, height: targetLayout.height };
}
