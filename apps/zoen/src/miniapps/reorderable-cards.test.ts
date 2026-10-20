import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { ReorderableAppCards } from './reorderable-cards';
import type { ItemDetail } from '../core/generated/ItemDetail';
type Element = { type: unknown; props: Record<string, unknown> };
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
type Gesture = Record<string, (...args: never[]) => unknown>;
const exportsByPort: Record<string, string[]> = {
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
  '../theme': ['useTheme'],
  '../i18n': ['t'],
};
const compiled = await build({
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
const require = createRequire(import.meta.url);
const tick = async () => {
  for (let i = 0; i < 24; i++) await Promise.resolve();
};
function fixture(reducedMotion = false, queuedScheduler = false) {
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
  const scrolls: number[] = [];
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
      withSpring: (value: unknown) => value,
      withTiming: (value: unknown) => value,
    },
    'react-native-worklets': {
      scheduleOnRN: (read: (...args: unknown[]) => unknown, ...args: unknown[]) => {
        if (queuedScheduler) scheduled.push(() => read(...args));
        else read(...args);
      },
    },
    './compact-card': { CompactMiniappCard: 'Card' },
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
  const style = () => {
    const animated = childTree.props.children as Element;
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
      nativeFrame: (childTree.props.children as Element).props.style as Record<string, unknown>[],
    }),
    scrollPort,
    render,
    card,
    style,
    writes,
    announcements,
    scrolls,
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

test('real compiled card gesture enters editing, suppresses trailing launch, and writes once on drop', async () => {
  const f = fixture();
  f.activate();
  assert.equal(f.editing, true);
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 0);
  f.move(325);
  assert.equal(f.writes.length, 0);
  f.release();
  await tick();
  f.render();
  assert.deepEqual(f.writes, [{ next: ['b', 'c', 'a'], expected: ['a', 'b', 'c'] }]);
  assert.ok(f.haptics >= 2);
  assert.deepEqual(f.announcements, ['home.rearranging', 'home.movedPosition']);
});
test('compiled reorder bounds use the same fixed square and 170 pt stride in rails and grids', async () => {
  const f = fixture();
  assert.equal(f.geometry().container.width, 500);
  assert.equal(f.geometry().container.height, 160);
  for (const card of f.geometry().cards) {
    assert.equal(card.size, 160);
    assert.equal(card.strideX, 170);
    assert.equal(card.strideY, 170);
    assert.equal(card.onMeasure, undefined);
  }
  assert.equal(f.geometry().nativeFrame[0]!.width, 160);
  assert.equal(f.geometry().nativeFrame[0]!.height, 160);
  Object.assign(f.props, { horizontal: false, columns: 2 });
  f.render();
  assert.equal(f.geometry().container.width, 330);
  assert.equal(f.geometry().container.height, 330);
  assert.equal(f.geometry().container.alignSelf, 'center');
  const beforeHalf = fixture();
  beforeHalf.activate();
  beforeHalf.move(84);
  beforeHalf.release();
  await tick();
  assert.deepEqual(beforeHalf.writes, []);
  const afterHalf = fixture();
  afterHalf.activate();
  afterHalf.move(86);
  afterHalf.release();
  await tick();
  assert.deepEqual(afterHalf.writes, [{ next: ['b', 'a', 'c'], expected: ['a', 'b', 'c'] }]);
});
test('real compiled canceled drag restores geometry and never persists', async () => {
  const f = fixture();
  f.activate();
  f.move(325);
  f.release(true);
  await tick();
  assert.equal(f.writes.length, 0);
  assert.equal(f.style().transform[0]!.translateX, 0);
});
test('background or route retirement cancels gesture and late drop/press callbacks', async () => {
  for (const retire of ['background', 'blur', 'unmount']) {
    const f = fixture();
    f.activate();
    f.move(325);
    const press = f.card().props.onPress as () => void;
    if (retire === 'background') f.state('background');
    else if (retire === 'blur') f.root.blur();
    else f.root.unmount();
    f.release();
    press();
    await tick();
    assert.equal(f.writes.length, 0, retire);
    assert.equal(f.opens, 0, retire);
    assert.equal(f.editing, false, retire);
  }
});
test('changed larger order scope or layout cancels a held card before drop', async () => {
  for (const change of ['context', 'geometry', 'membership']) {
    const f = fixture();
    f.activate();
    f.move(325);
    if (change === 'context') Object.assign(f.props, { contextKey: 'new-full-Home-order' });
    else if (change === 'geometry') Object.assign(f.props, { horizontal: false, columns: 2 });
    else f.props.items = f.props.items.slice(0, 2);
    f.render();
    f.release();
    await tick();
    assert.equal(f.writes.length, 0, change);
  }
});
test('denied deferred save rolls back and retirement cannot reopen editing on late acknowledgment', async () => {
  for (const retire of [false, true]) {
    let reject!: (error: Error) => void;
    const f = fixture();
    f.hold(
      new Promise((_yes, no) => {
        reject = no;
      }),
    );
    f.activate();
    f.move(325);
    f.release();
    if (retire) f.state('inactive');
    reject(new Error('denied'));
    await tick();
    f.render();
    assert.equal(f.style().transform[0]!.translateX, 0);
    assert.equal(f.editing, !retire);
  }
});
test('Reduce Motion has no rotation/lift while VoiceOver reorder still persists exact order', async () => {
  const f = fixture(true);
  f.activate();
  assert.equal(f.style().transform[2]!.scale, 1);
  assert.equal(f.style().transform[3]!.rotateZ, '0deg');
  f.release(true);
  f.render();
  const actions = f.card().props.accessibilityActions as { name: string }[];
  assert.deepEqual(
    Array.from(actions, (action) => action.name),
    ['moveLater'],
  );
  (f.card().props.onAccessibilityAction as (event: unknown) => void)({
    nativeEvent: { actionName: 'moveLater' },
  });
  await tick();
  assert.deepEqual(f.writes, [{ next: ['b', 'a', 'c'], expected: ['a', 'b', 'c'] }]);
});
test('real frame callback scrolls only near an edge and clamps the rail extent', () => {
  const f = fixture();
  f.activate();
  f.move(0, 150);
  f.root.frame();
  assert.deepEqual(f.scrolls, []);
  f.move(0, 299);
  f.root.frame();
  assert.ok(f.scrolls[0]! > 0);
  f.scrollPort.offset.set(299);
  f.root.frame();
  assert.equal(f.scrollPort.offset.get(), 300);
  f.release(true);
  f.root.frame();
  assert.equal(f.scrollPort.offset.get(), 300);
});

test('Done ends the jiggle and preserves the next ordinary tap, while the drag tail cannot launch', () => {
  const f = fixture();
  f.activate();
  f.release(true);
  f.render();
  assert.notEqual(f.style().transform[3]!.rotateZ, '0deg');
  f.done();
  assert.equal(f.style().transform[3]!.rotateZ, '0deg');
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 0);
  f.child.gesture('onTouchesDown');
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 1);
});

test('VoiceOver activates an ordinary card without a physical pan-down and cannot launch while editing', () => {
  const f = fixture();
  (f.card().props.onAccessibilityTap as () => void)();
  assert.equal(f.opens, 1);
  f.activate();
  (f.card().props.onAccessibilityTap as () => void)();
  assert.equal(f.opens, 1);
});

test('queued native begin and drop remain valid in the normal deferred journey', async () => {
  const f = fixture(false, true);
  f.activate();
  f.move(325);
  f.release();
  assert.equal(f.editing, false);
  assert.equal(f.writes.length, 0);
  assert.ok(f.scheduledCount >= 2);
  f.flush();
  await tick();
  f.render();
  assert.equal(f.editing, true);
  assert.deepEqual(f.writes, [{ next: ['b', 'c', 'a'], expected: ['a', 'b', 'c'] }]);
});
test('queued native begin cannot reopen editing after blur/refocus or background/resume', () => {
  for (const boundary of ['blur', 'background']) {
    const f = fixture(false, true);
    f.activate();
    assert.equal(f.editing, false);
    if (boundary === 'blur') {
      f.root.blur();
      f.root.focus();
    } else {
      f.state('background');
      f.state('active');
    }
    f.flush();
    f.render();
    assert.equal(f.editing, false, boundary);
    assert.deepEqual(f.announcements, [], boundary);
    assert.equal(f.writes.length, 0, boundary);
  }
});
test('queued native begin is invalidated by same-ID geometry or full-order context changes', () => {
  for (const boundary of ['geometry', 'context']) {
    const f = fixture(false, true);
    f.activate();
    if (boundary === 'geometry') Object.assign(f.props, { horizontal: false, columns: 2 });
    else Object.assign(f.props, { contextKey: 'changed-full-order' });
    f.render();
    f.flush();
    f.render();
    assert.equal(f.editing, false, boundary);
    assert.equal(f.writes.length, 0, boundary);
  }
});
test('queued native drop cannot persist after same-ID geometry or full-order context changes', async () => {
  for (const boundary of ['geometry', 'context']) {
    const f = fixture(false, true);
    f.activate();
    f.flush();
    f.render();
    assert.equal(f.editing, true);
    f.move(325);
    f.release();
    assert.equal(f.writes.length, 0);
    if (boundary === 'geometry') Object.assign(f.props, { horizontal: false, columns: 2 });
    else Object.assign(f.props, { contextKey: 'changed-full-order' });
    f.render();
    f.flush();
    await tick();
    f.render();
    assert.equal(f.writes.length, 0, boundary);
    assert.equal(f.style().transform[0]!.translateX, 0, boundary);
  }
});
test('a pending native edit admission also suppresses a new physical tap before RN receives begin', () => {
  const f = fixture(false, true);
  f.activate();
  f.release(true);
  f.child.gesture('onTouchesDown');
  (f.card().props.onPress as () => void)();
  (f.card().props.onAccessibilityTap as () => void)();
  assert.equal(f.opens, 0);
  f.flush();
  f.render();
  assert.equal(f.editing, true);
});
test('a newer native lift supersedes queued callbacks from the previous activation', async () => {
  const f = fixture(false, true);
  f.activate();
  f.move(325);
  f.release();
  f.activate();
  f.release(true);
  f.flush();
  await tick();
  f.render();
  assert.equal(f.editing, true);
  assert.equal(f.writes.length, 0);
  assert.deepEqual(f.announcements, ['home.rearranging']);
});
test('retirement never reopens editing when an already-admitted preference write later succeeds', async () => {
  let resolve!: () => void;
  const f = fixture(false, true);
  f.hold(
    new Promise((yes) => {
      resolve = () => yes(undefined);
    }),
  );
  f.activate();
  f.flush();
  f.render();
  f.move(325);
  f.release();
  f.flush();
  assert.equal(f.writes.length, 1);
  f.root.blur();
  f.root.focus();
  f.render();
  resolve();
  await tick();
  f.render();
  assert.equal(f.editing, false);
  assert.equal(f.writes.length, 1); // An admitted durable write is not canceled or rolled back here.
});
