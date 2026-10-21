import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const sources = {
  image: ['attachment-image-surface', 'AttachmentImageSurface', 'ZOEN_ATTACHMENT_IMAGE_SOURCE'],
  surface: [
    'attachment-gallery-surface',
    'AttachmentGallerySurface',
    'ZOEN_ATTACHMENT_SURFACE_SOURCE',
  ],
} as const;
const outputs = new Map<string, string>();
for (const [kind, [file, , override]] of Object.entries(sources)) {
  const path = fileURLToPath(new URL(`../media/${file}.tsx`, import.meta.url));
  const code = transformSync(readFileSync(process.env[override] ?? path, 'utf8'), {
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
  }).code as string;
  assert.match(code, /react\/compiler-runtime/, 'Actual installed iOS Babel/React compiler');
  const output = await build({
    stdin: { contents: code, resolveDir: dirname(path), loader: 'js' },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'gallery-native-ports',
        setup(b) {
          b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  });
  outputs.set(kind, output.outputFiles[0]!.text);
}
type Element = { type: any; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
function scan(value: any): Element[] {
  if (Array.isArray(value)) return value.flatMap(scan);
  return value?.props ? [value, ...scan(value.props.children), ...scan(value.props.overlay)] : [];
}
/** Components are production iOS compiler output; native UIScrollView/recognizers are explicit ports. */
function fixture(kind: keyof typeof sources) {
  const slots: Slot[] = [],
    caches: unknown[][] = [];
  let cursor = 0,
    cacheCursor = 0,
    effects: (() => void)[] = [],
    tree: Element,
    current = true;
  let taps: any[] = [],
    scrollHandler: ((event: any) => void) | undefined;
  const zoomCommands: any[] = [],
    scrollCommands: any[] = [],
    calls: any[] = [],
    timing: any[] = [];
  const props: any =
    kind === 'image'
      ? {
          identity: 'photo-one',
          uri: 'file:///owned/one.jpg',
          label: 'One',
          active: true,
          disabled: false,
          reducedMotion: false,
          isCurrent: () => current,
          onZoomChange: (id: string, zoomed: boolean) => calls.push(['zoom', id, zoomed]),
          onToggleControls: () => calls.push(['toggle']),
        }
      : {
          identity: 'photo-one',
          disabled: false,
          reducedMotion: false,
          controlsHidden: false,
          isCurrent: () => current,
          onClose: () => calls.push(['close']),
          children: 'same-pager',
          overlay: 'same-controls',
        };
  const slot = () => (slots[cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
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
      effects.push(() => {
        s.cleanup?.();
        s.cleanup = read() ?? undefined;
      });
    }
  };
  const shared = (value: any) => ({
    value,
    get() {
      return this.value;
    },
    set(next: any) {
      this.value = typeof next === 'function' ? next(this.value) : next;
    },
  });
  const modules: Record<string, any> = {
    react: {
      useRef: (initial: unknown) => {
        const s = slot();
        return (s.value ??= { current: initial });
      },
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value')) s.value = initial;
        return [
          s.value,
          (next: any) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
      useMemo: memo,
      useLayoutEffect: effect,
    },
    'react/compiler-runtime': {
      c(size: number) {
        const at = cacheCursor++;
        return (caches[at] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel')));
      },
    },
    'react/jsx-runtime': {
      jsx: (type: any, props: any) => ({ type, props }),
      jsxs: (type: any, props: any) => ({ type, props }),
    },
    'react-native': { View: 'View', ScrollView: 'ScrollView' },
    'expo-image': { Image: 'Image' },
    'react-native-gesture-handler': {
      InterceptingGestureDetector: 'InterceptingDetector',
      VirtualGestureDetector: 'VirtualDetector',
      useTapGesture: (config: any) => {
        taps.push(config);
        return config;
      },
      useExclusiveGestures: (...items: any[]) => items,
    },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView', ScrollView: 'AnimatedScrollView' },
      useSharedValue: (value: any) => {
        const s = slot();
        return (s.value ??= shared(value));
      },
      useAnimatedStyle: (read: any) => read(),
      useAnimatedScrollHandler: (read: any) => {
        scrollHandler = read;
        return read;
      },
      Easing: { linear: 'linear' },
      withTiming: (value: any, options: any) => {
        timing.push(options);
        return value;
      },
    },
  };
  const module = { exports: {} as Record<string, (props: any) => Element> };
  runInNewContext(outputs.get(kind)!, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/')) return require(name);
      assert.ok(name in modules, name);
      return modules[name];
    },
    global: { Error },
    console,
  });
  const render = (patch: Record<string, any> = {}) => {
    Object.assign(props, patch);
    cursor = cacheCursor = 0;
    effects = [];
    taps = [];
    tree = module.exports[sources[kind][1]]!(props);
    for (const node of scan(tree))
      if (node.props.ref)
        node.props.ref.current = {
          scrollResponderZoomTo: (...args: any[]) => zoomCommands.push(clone(args)),
          scrollTo: (args: any) => scrollCommands.push(clone(args)),
        };
    for (const run of effects) run();
    return tree;
  };
  const find = (typeOrId: string) => {
    const result = scan(tree).find((n) => n.type === typeOrId || n.props.testID === typeOrId);
    assert.ok(result, typeOrId);
    return result;
  };
  render();
  return {
    render,
    find,
    props,
    calls,
    zoomCommands,
    scrollCommands,
    timing,
    taps: () => taps,
    measure(width: number, height: number) {
      const target = scan(tree).find((n) => n.props.onLayout)!;
      target.props.onLayout({ nativeEvent: { layout: { width, height } } });
      render();
    },
    move(y: number) {
      scrollHandler?.({ contentOffset: { x: 0, y } });
      render();
    },
    drag(y: number, velocity = 0) {
      const scroll = find('attachment-gallery-dismiss');
      scroll.props.onScrollBeginDrag();
      scroll.props.onScrollEndDrag({
        nativeEvent: { contentOffset: { x: 0, y }, velocity: { x: 0, y: velocity } },
      });
    },
    retire() {
      current = false;
    },
    unmount() {
      for (const s of slots) s.cleanup?.();
    },
    clear() {
      calls.length = zoomCommands.length = scrollCommands.length = timing.length = 0;
    },
  };
}

test('compiled image uses actual native zoom-to-rect, locks its parent immediately and honors reduced motion', () => {
  const f = fixture('image');
  f.measure(402, 874);
  f.clear();
  f.taps()[0].onDeactivate({ canceled: false, x: 201, y: 437 });
  assert.deepEqual(f.calls, [['zoom', 'photo-one', true]]);
  assert.equal(f.zoomCommands.length, 1);
  assert.equal(f.zoomCommands[0][0].width, 134);
  assert.equal(f.zoomCommands[0][0].height, 874 / 3);
  assert.equal(f.zoomCommands[0][1], true);
  f.render({ reducedMotion: true });
  f.taps()[0].onDeactivate({ canceled: false, x: 201, y: 437 });
  assert.deepEqual(f.zoomCommands.at(-1), [
    { x: 0, y: 0, width: 402, height: 874, animated: false },
    false,
  ]);
});

test('image pinch/pan and corrected decoded aspect stay native while stale decode/selection callbacks cannot target newer bytes', () => {
  const f = fixture('image');
  f.measure(402, 874);
  const oldImage = f.find('Image'),
    oldScroll = f.find('ScrollView'),
    oldTap = f.taps()[0];
  f.render({ uri: 'file:///owned/replacement.jpg' });
  f.clear();
  oldImage.props.onLoad({ source: { url: 'file:///owned/one.jpg', width: 10000, height: 10 } });
  oldScroll.props.onScroll({ nativeEvent: { zoomScale: 2 } });
  oldTap.onDeactivate({ canceled: false, x: 30, y: 30 });
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.zoomCommands, []);
  f.find('Image').props.onLoad({
    source: { url: 'file:///owned/replacement.jpg', width: 1000, height: 100 },
  });
  f.render();
  assert.ok(f.find('ScrollView').props.maximumZoomScale > 20);
  f.find('ScrollView').props.onScroll({ nativeEvent: { zoomScale: 0.95 } });
  f.render();
  assert.equal(
    f.find('ScrollView').props.scrollEnabled,
    true,
    'a still-bouncing pinch keeps parent paging locked',
  );
  assert.deepEqual(f.calls.at(-1), ['zoom', 'photo-one', true]);
});

test('inactive, retired and unmounted image callbacks cannot zoom or toggle controls; orientation resets only native presentation', () => {
  const f = fixture('image');
  f.measure(402, 874);
  const held = f.taps()[0],
    load = f.find('Image').props.onLoad;
  f.render({ active: false });
  f.clear();
  held.onDeactivate({ canceled: false, x: 20, y: 30 });
  load({ source: { url: f.props.uri, width: 200, height: 100 } });
  assert.deepEqual(f.zoomCommands, []);
  f.render({ active: true });
  f.measure(874, 402);
  assert.deepEqual(f.zoomCommands.at(-1), [
    { x: 0, y: 0, width: 874, height: 402, animated: false },
    false,
  ]);
  const currentTap = f.taps()[1];
  f.retire();
  f.clear();
  currentTap.onDeactivate({ canceled: false });
  assert.deepEqual(f.calls, []);
  f.unmount();
  f.taps()[0].onDeactivate({ canceled: false, x: 10, y: 10 });
  assert.deepEqual(f.zoomCommands, []);
});

test('gallery native drag follows source background/controls distances and strict distance/velocity dismissal thresholds', () => {
  for (const sign of [-1, 1]) {
    const f = fixture('surface');
    f.measure(402, 874);
    f.clear();
    f.move(874 + sign * 40);
    const backing = f.find('AnimatedView');
    assert.equal(backing.props.style[1].opacity, 0.5);
    f.drag(874 + sign * (874 / 4));
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.scrollCommands.at(-1), { y: 874, animated: true });
    f.drag(874, sign * 1);
    assert.deepEqual(f.calls, []);
    f.drag(874, sign * 1.01);
    assert.deepEqual(f.calls, [['close']]);
    f.drag(874 + sign * 300);
    assert.equal(f.calls.length, 1, 'one native drag may close only once');
  }
});

test('programmatic or retained native end events cannot close after new selection, geometry, busy state or retirement', () => {
  const f = fixture('surface');
  f.measure(402, 874);
  let scroll = f.find('attachment-gallery-dismiss');
  scroll.props.onScrollEndDrag({ nativeEvent: { contentOffset: { y: 1200 }, velocity: { y: 3 } } });
  assert.deepEqual(f.calls, []);
  scroll.props.onScrollBeginDrag();
  const held = scroll.props.onScrollEndDrag;
  f.render({ identity: 'photo-two' });
  held({ nativeEvent: { contentOffset: { y: 1200 }, velocity: { y: 3 } } });
  assert.deepEqual(f.calls, []);
  scroll = f.find('attachment-gallery-dismiss');
  scroll.props.onScrollBeginDrag();
  const oldGeometry = scroll.props.onScrollEndDrag;
  f.measure(874, 402);
  f.measure(402, 874);
  oldGeometry({ nativeEvent: { contentOffset: { y: 1200 }, velocity: { y: 3 } } });
  assert.deepEqual(f.calls, []);
  f.render({ disabled: true });
  f.drag(1200, 3);
  assert.deepEqual(f.calls, []);
  f.render({ disabled: false });
  f.retire();
  f.drag(1200, 3);
  assert.deepEqual(f.calls, []);
  f.find('attachment-gallery-surface').props.onAccessibilityEscape();
  assert.deepEqual(f.calls, []);
});

test('controls fade and native recoil honor reduced motion without replacing the child or admitting a hidden old gesture', () => {
  const f = fixture('surface');
  f.measure(402, 874);
  f.clear();
  f.render({ controlsHidden: true, reducedMotion: true });
  assert.deepEqual(clone(f.timing.at(-1)), { duration: 0, easing: 'linear' });
  const overlay = scan(f.find('attachment-gallery-surface')).find(
    (n) =>
      n.type === 'AnimatedView' &&
      n.props.pointerEvents === 'none' &&
      n.props.accessibilityElementsHidden,
  );
  assert.ok(overlay);
  assert.equal(overlay.props.children, 'same-controls');
  f.drag(974);
  assert.deepEqual(f.scrollCommands.at(-1), { y: 874, animated: false });
  assert.ok(
    scan(f.find('attachment-gallery-dismiss')).some((n) => n.props.children === 'same-pager'),
  );
});
