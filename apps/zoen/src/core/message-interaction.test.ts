import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { MessageInteractionProps } from '../components/message-interaction.types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const path = fileURLToPath(new URL('../components/message-interaction.tsx', import.meta.url));
const compiled = transformSync(
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
const ports = new Set([
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
const output = await build({
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
type Element = { type: unknown; props: Record<string, unknown> };
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
type Hooks = {
  cursor: number;
  cacheCursor: number;
  slots: Slot[];
  caches: unknown[][];
  pending: (() => void)[];
};
const hooks = (): Hooks => ({ cursor: 0, cacheCursor: 0, slots: [], caches: [], pending: [] });
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
function styles(node: Element): Record<string, unknown> {
  const flatten = (value: unknown): Record<string, unknown> =>
    Array.isArray(value)
      ? Object.assign({}, ...value.map(flatten))
      : ((value ?? {}) as Record<string, unknown>);
  return flatten(node.props.style);
}
const json = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

/** Real Babel component/hooks execute. Native gesture, focus, animation and RN scheduling are explicit ports, not a device proof. */
function fixture(initial: Partial<MessageInteractionProps> = {}) {
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
    press() {
      let pressed = 0;
      const node = commit(child, () =>
        module.exports.MessagePressable({ onPress: () => ++pressed }),
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

function swipe(f: ReturnType<typeof fixture>, distance: number) {
  f.begin();
  f.move(Math.sign(distance) * 10);
  f.update(distance);
  f.end(distance);
}

test('real compiled gesture sends one directional intent beyond incoming/own threshold and one heavy impact', () => {
  for (const mine of [false, true])
    for (const direction of [-1, 1]) {
      const f = fixture({ mine });
      const threshold = mine ? 60 : 45;
      f.begin();
      f.move(direction * 10);
      f.update(direction * threshold);
      f.update(direction * (threshold + 1));
      f.update(0);
      f.update(-direction * (threshold + 10));
      f.end(direction * (threshold + 1));
      assert.equal(f.calls.length, 0, 'Native→RN action remains scheduled');
      f.flush();
      assert.deepEqual(f.calls, [direction < 0 ? 'reply' : 'thread']);
      assert.deepEqual(f.impacts, ['heavy']);
      assert.deepEqual(f.states, ['activate']);
    }
});

test('strict boundary, canceled drag and velocity-only flick never send an action', () => {
  for (const mine of [false, true])
    for (const canceled of [false, true]) {
      const f = fixture({ mine });
      f.begin();
      f.move(-10);
      f.update(mine ? -60 : -45);
      f.end(canceled ? -100 : mine ? -60 : -45, canceled, -2000);
      f.flush();
      assert.deepEqual(f.calls, []);
    }
  const flick = fixture();
  flick.begin();
  flick.move(-10);
  flick.update(-36);
  flick.end(-36, false, -2000);
  flick.flush();
  assert.deepEqual(flick.calls, []);
});

for (const boundary of ['unmount', 'row', 'disable', 'focus', 'background', 'activation'] as const)
  test(`queued action and impact retire across ${boundary} before RN dispatch`, () => {
    const f = fixture();
    swipe(f, -80);
    assert.equal(f.pending.length, 2);
    if (boundary === 'unmount') f.unmount();
    else if (boundary === 'row') f.render({ messageId: 'm2' });
    else if (boundary === 'disable') {
      f.render({ disabled: true });
      f.render({ disabled: false });
    } else if (boundary === 'focus') {
      f.blur();
      f.focus();
    } else if (boundary === 'background') {
      f.app('inactive');
      f.app('active');
    } else f.begin();
    f.flush();
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.impacts, []);
  });

test('scheduled intent uses the latest committed handler without inventing canonical authority', () => {
  const f = fixture();
  swipe(f, -80);
  f.render({ onReply: () => f.calls.push('latest') });
  f.flush();
  assert.deepEqual(f.calls, ['latest']);
});

test('back edge, vertical and multiple pointers fail admission without consuming normal press', () => {
  const f = fixture();
  f.begin(23);
  assert.deepEqual(f.states, ['fail']);
  assert.equal(f.press(), 1);
  f.begin();
  f.move(0, 20);
  assert.equal(f.states.at(-1), 'fail');
  f.begin();
  f.move(10, 0, 90, 2);
  assert.equal(f.states.at(-1), 'fail');
});

test('message pan and nested media consumption suppress trailing physical press, next touch rearms', () => {
  const f = fixture();
  f.begin();
  assert.equal(f.press(), 1);
  f.consume();
  assert.equal(f.press(), 0);
  f.begin();
  f.move(-10);
  f.end(-20);
  assert.equal(f.press(), 0);
  f.begin();
  assert.equal(f.press(), 1);
  f.blur();
  f.focus();
  assert.equal(f.press(), 0);
});

test('reduced motion keeps reply/thread accessible while silencing impact and displacement', () => {
  const f = fixture({
    accessibilityLabel: 'Message body',
    onActions: () => f.calls.push('actions'),
    actionsAccessibilityLabel: 'Actions',
  });
  f.reduced(true);
  swipe(f, -80);
  f.flush();
  assert.deepEqual(f.calls, ['reply']);
  assert.deepEqual(f.impacts, []);
  assert.deepEqual(f.animation, []);
  f.voiceOver(true);
  const row = elements(f.tree).find((item) => item.props.accessible === true)!;
  const action = row.props.onAccessibilityAction as (event: unknown) => void;
  action({ nativeEvent: { actionName: 'thread' } });
  assert.deepEqual(f.calls, ['reply', 'thread']);
  const movement = styles(row).transform as { translateX: number }[];
  assert.equal(movement[0]!.translateX, 0);
});

test('indicator shares moving row coordinate space, source 33pt geometry and independent release fade/recoil', () => {
  for (const mine of [false, true]) {
    const f = fixture({ mine });
    f.begin();
    f.move(-10);
    f.update(-20);
    f.render();
    const detector = elements(f.tree).find((item) => item.type === 'Detector')!;
    const moving = detector.props.children as Element;
    assert.equal(styles(moving).width, '100%');
    assert.equal((styles(moving).transform as { translateX: number }[])[0]!.translateX, -20);
    const reply = f.indicator('reply');
    const style = styles(reply);
    assert.equal(style.width, 33);
    assert.equal(style.height, 33);
    assert.equal(style.right, mine ? -43 : -9);
    const thread = f.indicator('thread');
    assert.equal(styles(thread).left, -43);
    f.end(-20);
    const recoil = f.animation.filter(
      (item) => (item.options as { duration: number }).duration === 300,
    );
    assert.equal(recoil.length, 2);
    assert.deepEqual(json(recoil.map((item) => item.options)), [
      { duration: 300, easing: [0.38, 0.7, 0.125, 1] },
      { duration: 300, easing: [0.38, 0.7, 0.125, 1] },
    ]);
  }
});

// A retained native configuration cannot acquire the new row epoch after recycling.
test('old native worklet entry and finalization cannot activate or reset a recycled row', () => {
  const f = fixture();
  const previous = f.pan;
  f.render({ messageId: 'm2' });
  previous.onTouchesDown!({
    numberOfTouches: 1,
    handlerTag: 9,
    allTouches: [{ absoluteX: 90, absoluteY: 100 }],
  });
  previous.onTouchesMove!({
    numberOfTouches: 1,
    handlerTag: 9,
    allTouches: [{ absoluteX: 70, absoluteY: 100 }],
  });
  previous.onUpdate!({ translationX: -80 });
  previous.onDeactivate!({ translationX: -80, canceled: false });
  previous.onFinalize!({});
  f.flush();
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.impacts, []);
  assert.deepEqual(f.states, ['fail']);
  swipe(f, -80);
  f.flush();
  assert.deepEqual(f.calls, ['reply']);
});

for (const boundary of ['row', 'disable', 'focus', 'background', 'activation'] as const)
  test(`old scheduled finish cannot borrow a fresh completed gesture after ${boundary}`, () => {
    const f = fixture();
    swipe(f, -80);
    if (boundary === 'row') f.render({ messageId: 'm2' });
    else if (boundary === 'disable') {
      f.render({ disabled: true });
      f.render({ disabled: false });
    } else if (boundary === 'focus') {
      f.blur();
      f.focus();
    } else if (boundary === 'background') {
      f.app('inactive');
      f.app('active');
    }
    swipe(f, 80);
    f.flush();
    assert.deepEqual(f.calls, ['thread']);
    assert.deepEqual(f.impacts, ['heavy']);
  });

test('changing reduced motion before queued impact quiets it without rearming the gesture', () => {
  const f = fixture();
  f.begin();
  f.move(-10);
  f.update(-60);
  f.reduced(true);
  f.flush();
  assert.deepEqual(f.impacts, []);
  f.reduced(false);
  f.update(-80);
  f.end(-80);
  f.flush();
  assert.deepEqual(f.impacts, []);
  assert.deepEqual(f.calls, ['reply']);
});
