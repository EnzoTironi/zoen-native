import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { quoteMotionCurve } from './quote-motion.ts';
import { InitialTimelineEnd } from './initial-timeline-end.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const ports = new Set([
  'react',
  'react/jsx-runtime',
  'react/compiler-runtime',
  'react-native-worklets',
  'react-native',
  'react-native-reanimated',
  'react-native-enriched-markdown',
  '../theme',
  './message-interaction',
  '../i18n/index.ts',
  './components/CompatView',
  './RecyclerViewContextProvider',
]);
const compiled = await build({
  stdin: {
    contents: `export {QuoteLayoutTransition} from './src/components/quote-layout-transition'; export {QuoteLayout,QuoteMotionView} from './src/components/quote-layout-context'; export {useQuoteCollapse} from './src/components/use-quote-collapse'; export {ViewHolderCollection} from './node_modules/@shopify/flash-list/src/recyclerview/ViewHolderCollection';`,
    resolveDir: process.cwd(),
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
      name: 'maintained-flashlist-motion',
      setup(b) {
        b.onLoad(
          { filter: /\/(quote-layout-transition\.tsx|use-quote-collapse\.ts|quote-motion\.ts)$/ },
          ({ path }) => ({
            contents: transformSync(readFileSync(path, 'utf8'), {
              filename: path,
              babelrc: false,
              configFile: false,
              presets: ['babel-preset-expo'],
              caller: {
                name: 'metro',
                platform: 'ios',
                engine: 'hermes',
                isDev: false,
                supportsReactCompiler: true,
              },
            }).code,
            loader: 'js',
          }),
        );
        b.onResolve({ filter: /.*/ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

test('actual FlashList commit starts one shared quote transition, with retargeting and reduced motion', async () => {
  const root = createReactTestRoot();
  const cells = new Map<number, any>();
  let policy: any;
  let revision = 'rev1';
  let quote: any,
    reduced = false,
    prepared = 0;
  const timings: any[] = [];
  const frames = new Set<() => void>();
  let endScrolls = 0;
  const initialEnd = new InitialTimelineEnd(
    'quote-chat',
    {
      current: () => true,
      frame: (work) => {
        frames.add(work);
        return () => {
          frames.delete(work);
        };
      },
      scroll: () => {
        endScrolls++;
        return true;
      },
    },
    { anchorUntilInteraction: true },
  );
  const flushEnd = () => {
    for (const work of frames) {
      frames.delete(work);
      work();
    }
  };
  initialEnd.focus();
  initialEnd.onLoad();
  initialEnd.onViewportLayout(650);
  initialEnd.onFooterLayout(86);
  initialEnd.onContentSizeChange(400, 1200);
  flushEnd();
  assert.equal(endScrolls, 1, 'opening still reaches the initial end');
  const shared: any[] = [];
  function Host(props: any) {
    if (props.index !== undefined) cells.set(props.index, props);
    return props.children ?? null;
  }
  function value(initial: any) {
    let current = initial;
    const ref: any = {};
    Object.defineProperty(ref, 'value', {
      get: () => current,
      set: (next) => {
        if (next?.timing)
          timings.push({
            ...next,
            assign: (x: any) => {
              current = x;
            },
          });
        else current = next;
      },
    });
    ref.get = () => ref.value;
    ref.set = (next: any) => {
      ref.value = next;
    };
    shared.push(ref);
    return ref;
  }
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    global: { Error },
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/jsx-runtime' || name === 'react/compiler-runtime') return require(name);
      if (name === 'react-native-worklets') return { createSerializable: (x: any) => x };
      if (name === 'react-native')
        return {
          View: Host,
          useWindowDimensions: () => ({ width: 400, height: 800, fontScale: 1 }),
        };
      if (name === 'react-native-enriched-markdown') return { EnrichedMarkdownText: Host };
      if (name === './message-interaction') return { MessagePressable: Host };
      if (name === '../theme') return { useTheme: () => ({ reducedMotion: reduced }) };
      if (name === '../i18n/index.ts') return { t: (x: string) => x };
      if (name === './components/CompatView') return { CompatView: Host };
      if (name === './RecyclerViewContextProvider') return { useRecyclerViewContext: () => null };
      if (name === 'react-native-reanimated')
        return {
          __esModule: true,
          default: { View: Host, createAnimatedComponent: (x: any) => x },
          defineAnimation: (_: any, f: any) => f(),
          runOnUI: (f: any) => f,
          runOnJS: (f: any) => f,
          useAnimatedProps: (f: any) => ({ read: f }),
          useSharedValue: (initial: any) => {
            const ref = React.useRef<any>(null);
            if (!ref.current) ref.current = value(initial);
            return ref.current;
          },
          withTiming: (to: any, config: any, callback: any) => ({
            timing: true,
            to,
            config,
            callback,
          }),
        };
      throw Error(name);
    },
  });
  const { QuoteLayoutTransition, QuoteMotionView, useQuoteCollapse, ViewHolderCollection } =
    module.exports;
  const list = {
    current: {
      prepareForLayoutAnimationRender() {
        prepared++;
      },
    },
  };
  const collection = React.createRef<any>();
  let layouts = [
    { x: 0, y: 0, width: 200, height: 100 },
    { x: 0, y: 100, width: 200, height: 100 },
  ];
  function Probe() {
    const current = useQuoteCollapse('> one\n> two\n> three\n> four', 'message-1', revision, false);
    React.useLayoutEffect(() => {
      quote = current;
    }, [current]);
    return null;
  }
  function Policy(props: any) {
    React.useLayoutEffect(() => {
      policy = props;
    }, [props]);
    return null;
  }
  const render = () =>
    root.render(
      React.createElement(QuoteLayoutTransition, {
        list,
        onBegin: initialEnd.cancel,
        // oxlint-disable-next-line react/no-children-prop -- actual render-prop API under test
        children: (active: boolean, commit: () => void, cancel: () => void, reading: boolean) =>
          React.createElement(
            React.Fragment,
            null,
            React.createElement(Probe),
            React.createElement(Policy, { active, cancel, reading }),
            React.createElement(ViewHolderCollection, {
              data: ['a', 'b'],
              renderStack: new Map([
                ['a', { index: 0 }],
                ['b', { index: 1 }],
              ]),
              getLayout: (i: number) => layouts[i],
              getChildContainerLayout: () => ({ width: 200, height: layouts[1]!.y + 100 }),
              getAdjustmentMargin: () => 0,
              isInLastRow: () => false,
              viewHolderCollectionRef: collection,
              refHolder: new Map(),
              renderItem: () => null,
              CellRendererComponent: QuoteMotionView,
              onCommitLayoutEffect: commit,
              onCommitEffect: () => {},
              extraData: active,
            }),
          ),
      }),
    );
  try {
    await render();
    assert.equal(timings.length, 0, 'no entrance');
    assert.equal(policy.reading, false);
    await root.act(() => collection.current.commitLayout());
    assert.equal(timings.length, 0, 'ordinary list commit is instant');
    const generation = quote.quoteCollapse.generation;
    await root.act(() => quote.onBlockquoteToggle({ nativeEvent: { index: 0, generation } }));
    assert.equal(prepared, 1);
    initialEnd.onContentSizeChange(400, 3000);
    initialEnd.onCommitLayoutEffect();
    flushEnd();
    assert.equal(
      endScrolls,
      1,
      'quote intent retires initial-end follow before enlarged layout commits',
    );
    assert.equal(policy.active, true);
    assert.equal(policy.reading, true);
    assert.equal(timings.length, 0, 'tap/React render cannot start the clock');
    assert.equal(
      quote.quoteCollapse.expanded[0],
      0,
      'canonical target expansion already committed',
    );
    layouts = [
      { x: 0, y: 0, width: 200, height: 300 },
      { x: 0, y: 300, width: 200, height: 100 },
    ];
    await root.act(() => collection.current.commitLayout());
    assert.equal(timings.length, 1, 'actual maintained FlashList commit starts motion');
    assert.equal(timings[0].config.duration, 400);
    const layout = cells.get(1).layout;
    const frame = layout({
      currentOriginX: 0,
      currentOriginY: 100,
      currentWidth: 200,
      currentHeight: 100,
      targetOriginX: 0,
      targetOriginY: 300,
      targetWidth: 200,
      targetHeight: 100,
    });
    const y = frame.animations.originY;
    y.onStart(y, 100);
    y.onFrame(y);
    assert.equal(y.current, 100);
    timings[0].assign(0.5);
    y.onFrame(y);
    assert.equal(y.current, 200, 'neighbour uses the one shared progress');
    const late = layout({
      currentOriginX: 0,
      currentOriginY: 20,
      currentWidth: 200,
      currentHeight: 100,
      targetOriginX: 0,
      targetOriginY: 40,
      targetWidth: 200,
      targetHeight: 100,
    }).animations.originY;
    late.onStart(late, 20);
    late.onFrame(late);
    assert.equal(late.current, 30, 'late canonical position joins same progress');
    await root.act(() => quote.onBlockquoteToggle({ nativeEvent: { index: 0, generation } }));
    y.onFrame(y);
    assert.equal(y.current, 200, 'interruption begins at current presentation');
    await root.act(() => collection.current.commitLayout());
    assert.equal(timings.length, 2);
    timings[0].callback(true);
    assert.equal(shared[1].value.active, true, 'old completion cannot finish new owner');
    reduced = true;
    await render();
    y.onFrame(y);
    assert.equal(y.current, 300, 'Reduce Motion finishes canonical target');
    await root.act(() => quote.onBlockquoteToggle({ nativeEvent: { index: 0, generation } }));
    assert.equal(prepared, 3, 'reduced motion still prepares reading preservation');
    assert.equal(timings.length, 2, 'reduced motion arms no transition');
    assert.equal(policy.active, false);
    assert.equal(
      policy.reading,
      true,
      'completion preserves explicit reading intent instead of bottom-follow',
    );
    await root.act(() => policy.cancel());
    assert.equal(policy.reading, false, 'physical scrolling releases the quote reading policy');
    reduced = false;
    await render();
    await root.act(() => quote.onBlockquoteToggle({ nativeEvent: { index: 0, generation } }));
    assert.equal(shared[1].value.active, true);
    revision = 'rev2';
    await render();
    assert.equal(shared[1].value.active, false, 'new canonical revision finishes old presentation');
    assert.equal(shared[0].value, 1);
    assert.equal(
      quote.quoteCollapse.expanded.length,
      0,
      'revision never inherits an old expansion',
    );
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});

test('public spring projection has stable endpoints and no entrance overshoot', () => {
  assert.equal(quoteMotionCurve(0), 0);
  assert.equal(quoteMotionCurve(1), 1);
  let previous = 0;
  for (let i = 0; i <= 100; i++) {
    const p = quoteMotionCurve(i / 100);
    assert.ok(p >= previous && p <= 1);
    previous = p;
  }
});
