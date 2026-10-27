import { MessageTextSelectionSession } from '../components/message-text-selection.ts';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
const compiled = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/message-context-source.ios.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'context-source-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
type Node = { type: unknown; props: Record<string, any> };
function fixture(options: { available?: boolean; staged?: boolean } = {}) {
  let uuid = 0,
    cursor = 0,
    stateCursor = 0,
    scopeKey: string | undefined,
    focused = false;
  const stateSlots: unknown[] = [];
  const refs: { current: any }[] = [],
    layouts: (() => void)[] = [];
  let focusCleanup: (() => void) | undefined;
  let layoutCleanup: (() => void) | undefined;
  let background: (state: string) => void = () => {};
  const requests: any[] = [];
  const owns = {},
    stage = {};
  const module = { exports: {} as { MessageContextSource(props: any): Node | string } };
  const ports: Record<string, any> = {
    react: {
      useSyncExternalStore: (_subscribe: unknown, read: () => unknown) => read(),
      useRef: (value: unknown) => (refs[cursor++] ??= { current: value }),
      useState: (initial: unknown) => {
        const index = stateCursor++;
        if (!(index in stateSlots))
          stateSlots[index] = typeof initial === 'function' ? initial() : initial;
        return [stateSlots[index], () => {}];
      },
      use: (context: unknown) => context === stage && options.staged !== false,
      useCallback: (callback: unknown) => callback,
      useLayoutEffect: (effect: () => void | (() => void)) =>
        layouts.push(() => {
          layoutCleanup?.();
          layoutCleanup = effect() ?? undefined;
        }),
      useEffect: (effect: () => void) => {
        if (!backgroundSet) {
          backgroundSet = true;
          effect();
        }
      },
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: Record<string, unknown>) => ({ type, props }),
    },
    'react-native': {
      AppState: {
        currentState: 'active',
        addEventListener: (_event: string, callback: typeof background) => {
          background = callback;
          return { remove() {} };
        },
      },
    },
    'expo-router': {
      useFocusEffect: (effect: () => () => void) => {
        if (!focused) {
          focused = true;
          focusCleanup = effect();
        }
      },
    },
    expo: {
      requireOptionalNativeModule: () => ({
        messageContextVersion: options.available === false ? 0 : 2,
      }),
      requireNativeView: (_module: string, name: string) => name,
    },
    'expo-crypto': { randomUUID: () => `uuid-${++uuid}` },
    './message-text-selection': { MessageTextSelectionSession },
    './message-context-control': { MessageContextControl: 'ContextControl' },
    './message-context-ports': {
      MessageContextOwnsPress: owns,
      MessageContextStageEnabled: stage,
      MessageTextSelection: {},
    },
  };
  let backgroundSet = false;
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const native = {
    presentContext() {},
    dismissContext() {},
    consumeGesture: async () => true,
    finishTextSelection: async () => {},
  };
  const render = (patch: object = {}) => {
    const outer = module.exports.MessageContextSource({
      ownerKey: 'scope:message',
      active: true,
      handlesTap: true,
      children: 'same-live-bubble',
      onGesture: (request: unknown) => requests.push(request),
      ...patch,
    });
    let node = outer;
    if (typeof outer !== 'string' && typeof outer.type === 'function') {
      const nextScope = outer.props.ownerKey;
      if (scopeKey !== nextScope) {
        layoutCleanup?.();
        focusCleanup?.();
        if (refs[0]) refs[0].current = null;
        refs.length = 0;
        stateSlots.length = 0;
        focused = false;
        backgroundSet = false;
        layoutCleanup = undefined;
        focusCleanup = undefined;
        scopeKey = nextScope;
      }
      cursor = 0;
      stateCursor = 0;
      node = (outer.type as (props: any) => Node | string)(outer.props);
    }
    layouts.splice(0).forEach((effect) => effect());
    if (typeof node !== 'string') node.props.ref.current = native;
    return node;
  };
  const event = (node: Node, patch: object = {}) =>
    node.props.onGesture({
      nativeEvent: {
        binding: node.props.binding,
        gestureId: 'gesture',
        sequence: 1,
        kind: 'hold',
        ...patch,
      },
    });
  return {
    render,
    event,
    requests,
    native,
    owns,
    blur: () => {
      focusCleanup?.();
    },
    background: () => background('background'),
    unmount: () => {
      layoutCleanup?.();
    },
    replaceNative: () => {
      refs[0]!.current = null;
    },
  };
}

test('one native Source contains the SAME bubble; only its physical plain-bubble presses are owned', () => {
  const f = fixture(),
    node = f.render() as Node;
  assert.equal(node.type, 'MessageContextSource');
  assert.equal(node.props.children.type, f.owns);
  assert.equal(node.props.children.props.value, true);
  assert.equal(node.props.children.props.children.props.children, 'same-live-bubble');
  f.event(node);
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0].native, f.native);
  assert.equal(f.requests[0].current(), true);
});
test('media has no duplicate plain-bubble press owner', () => {
  const f = fixture(),
    node = f.render({ handlesTap: false }) as Node;
  assert.equal(node.props.children.props.value, false);
});
for (const boundary of ['blur', 'background', 'unmount', 'row', 'disabled'] as const)
  test(`actual compiled old native event and captured request cannot cross ${boundary}`, () => {
    const f = fixture(),
      old = f.render() as Node;
    f.event(old);
    const request = f.requests[0];
    if (boundary === 'row') f.render({ ownerKey: 'scope:new-message' });
    else if (boundary === 'disabled') f.render({ active: false });
    else f[boundary]();
    assert.equal(request.current(), false);
    f.event(old);
    assert.equal(f.requests.length, 1);
  });
for (const options of [{ available: false }, { staged: false }])
  test(`missing capability or root Stage preserves the original fallback (${JSON.stringify(options)})`, () => {
    const f = fixture(options);
    assert.equal(f.render(), 'same-live-bubble');
  });
test('foreign binding and unsupported native event never admit a request', () => {
  const f = fixture(),
    node = f.render() as Node;
  f.event(node, { binding: 'foreign' });
  f.event(node, { kind: 'invented' });
  assert.deepEqual(f.requests, []);
});

test('a newer gesture consumes the old event and invalidates its captured request', () => {
  const f = fixture(),
    node = f.render() as Node;
  f.event(node);
  const old = f.requests[0];
  f.event(node, { gestureId: 'new-gesture', sequence: 2 });
  assert.equal(old.current(), false);
  assert.equal(f.requests[1].current(), true);
  f.event(node, { gestureId: 'gesture', sequence: 1 });
  assert.equal(f.requests.length, 2);
});

test('custom reduced motion reaches the native Source on prop updates without replacing row identity', () => {
  const f = fixture();
  const initial = f.render() as Node;
  assert.equal(initial.props.reducedMotion, false);
  const reduced = f.render({ reducedMotion: true }) as Node;
  assert.equal(reduced.props.reducedMotion, true);
  assert.equal(reduced.props.binding, initial.props.binding);
  f.event(reduced);
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0].current(), true);
  const restored = f.render({ reducedMotion: false }) as Node;
  assert.equal(restored.props.reducedMotion, false);
  assert.equal(restored.props.binding, initial.props.binding);
});

test('owner-keyed React lifetime preserves binding on updates and fences old scope before new admission', () => {
  const f = fixture();
  const first = f.render({ ownerKey: 'scope:one' }) as Node;
  f.event(first);
  const oldRequest = f.requests[0];
  const update = f.render({ ownerKey: 'scope:one', reducedMotion: true }) as Node;
  assert.equal(update.props.binding, first.props.binding);
  const replacement = f.render({ ownerKey: 'scope:two' }) as Node;
  assert.notEqual(replacement.props.binding, first.props.binding);
  assert.equal(oldRequest.current(), false);
  f.event(first, { sequence: 2 });
  assert.equal(f.requests.length, 1);
  f.event(replacement);
  assert.equal(f.requests.length, 2);
  assert.equal(f.requests[1].current(), true);
});

test('selection phase retains exact source binding, owns native touches, and restores the same bubble', () => {
  const f = fixture();
  let node = f.render({ handlesTap: false }) as Node;
  f.event(node);
  const request = f.requests[0];
  let started = 0;
  request.textSelection.bind({
    started: () => {
      started++;
    },
    review: async () => '{}',
  });
  f.event(node, {
    kind: 'selection',
    sequence: 2,
    selection: { requestId: 'context', active: true },
  });
  node = f.render({ handlesTap: false }) as Node;
  assert.equal(started, 1);
  assert.equal(request.current(), true);
  assert.equal(f.requests.length, 1);
  assert.equal(node.props.children.props.value, true);
  assert.equal(node.props.children.props.children.props.value.active, true);
  assert.equal(node.props.children.props.children.props.children, 'same-live-bubble');
  f.event(node, {
    kind: 'selection',
    sequence: 3,
    selection: { requestId: 'context', active: false },
  });
  node = f.render({ handlesTap: false }) as Node;
  assert.equal(node.props.children.props.value, false);
  assert.equal(node.props.children.props.children.props.children, 'same-live-bubble');
});
