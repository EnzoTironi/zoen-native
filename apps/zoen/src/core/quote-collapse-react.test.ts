import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { elements, plainMessage, render as renderFace } from './message-caption.fixtures.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const app = fileURLToPath(new URL('../../', import.meta.url));
const pkg = app + 'node_modules/react-native-enriched-markdown/';
const ports = new Set([
  'react',
  'react/jsx-runtime',
  'react/compiler-runtime',
  'react-native',
  '../theme',
  '../i18n/index.ts',
  './use-link-launch',
  './use-message-code-copy',
  '../EnrichedMarkdownNativeComponent',
  '../EnrichedMarkdownTextNativeComponent',
]);
const compiled = await build({
  stdin: {
    contents: `export { MessageMarkdown } from './message-markdown'; export { MessageTextSelection } from './message-context-ports';`,
    resolveDir: app + 'src/components',
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
      name: 'actual-expo-quote-consumer',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (path === 'react-native-enriched-markdown')
            return { path, namespace: 'maintained-facade' };
          return ports.has(path) ? { path, external: true } : undefined;
        });
        builder.onLoad({ filter: /.*/, namespace: 'maintained-facade' }, () => ({
          contents: `export {default as EnrichedMarkdownText} from ${JSON.stringify(pkg + 'lib/module/native/EnrichedMarkdownText.js')};`,
          loader: 'js',
          resolveDir: app,
        }));
        builder.onLoad(
          { filter: /\/(message-markdown\.tsx|use-quote-collapse\.ts)$/ },
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
      },
    },
  ],
});
function fixture() {
  const root = createReactTestRoot();
  let native: any;
  const module = { exports: {} as any };
  function Native(props: any) {
    React.useLayoutEffect(() => {
      native = props;
    }, [props]);
    return null;
  }
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    __DEV__: false,
    require(name: string) {
      if (name === 'react') return React;
      if (
        name === 'react/jsx-runtime' ||
        name === 'react/compiler-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'react-native')
        return {
          Text: () => null,
          Platform: { OS: 'ios', select: (x: any) => x.ios ?? x.default },
          processColor: (x: any) => x,
        };
      if (
        name === '../EnrichedMarkdownNativeComponent' ||
        name === '../EnrichedMarkdownTextNativeComponent'
      )
        return Native;
      if (name === '../theme')
        return {
          useTheme: () => ({
            scheme: 'dark',
            colors: {
              text: '#fff',
              accent: '#09f',
              secondary: '#aaa',
              surface: '#111',
              line: '#333',
              muted: '#222',
            },
          }),
        };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === './use-link-launch') return { useLinkLaunch: () => () => {} };
      if (name === './use-message-code-copy') return { useMessageCodeCopy: () => () => {} };
      throw Error(name);
    },
  });
  return {
    root,
    get native() {
      return native;
    },
    async render(props: Record<string, unknown>, selecting = false) {
      await root.render(
        React.createElement(
          module.exports.MessageTextSelection,
          { value: { active: selecting } },
          React.createElement(module.exports.MessageMarkdown, {
            color: '#fff',
            fontSize: 17,
            maxWidth: 280,
            ...props,
          }),
        ),
      );
      assert.equal(root.errors.length, 0);
      return native;
    },
  };
}
const text = '> **café** 🦊 [link](https://example.org/a_(b))\n> two\n> three\n> four';
test('actual Expo consumer and maintained public facade keep one controlled quote state per message/revision', async () => {
  const f = fixture();
  try {
    const props = { text, testID: 'message-body-A', revision: 'r1' };
    let n = await f.render(props);
    assert.equal(n.markdown, text);
    assert.equal(n.isGFM, true);
    assert.equal(
      n.streamingAnimation,
      false,
      'no startup or expand animation in any motion preference',
    );
    assert.ok(n.quoteCollapse, 'native view must receive controlled quote presentation');
    assert.equal(n.quoteCollapse.enabled, true);
    assert.deepEqual([...n.quoteCollapse.expanded], []);
    const generation = n.quoteCollapse.generation;
    const original = n.onBlockquoteToggle;
    await f.root.act(() => original({ nativeEvent: { index: 0, generation } }));
    assert.deepEqual([...f.native.quoteCollapse.expanded], [0]);
    await f.root.act(() => f.native.onBlockquoteToggle({ nativeEvent: { index: 1, generation } }));
    await f.root.act(() => f.native.onBlockquoteToggle({ nativeEvent: { index: 0, generation } }));
    assert.deepEqual(
      [...f.native.quoteCollapse.expanded],
      [1],
      'the same action folds an expanded block',
    );
    assert.equal(f.native.markdown, text, 'folding never rewrites canonical content');
    await f.root.act(() => f.native.onBlockquoteToggle({ nativeEvent: { index: 0, generation } }));
    n = await f.render({ ...props, maxWidth: 190, fontSize: 28 });
    assert.equal(n.quoteCollapse.generation, generation);
    assert.deepEqual(
      [...n.quoteCollapse.expanded],
      [0, 1],
      'layout and Dynamic Type keep presentation',
    );
    n = await f.render({ ...props, expandedForReveal: true });
    assert.equal(
      n.quoteCollapse.enabled,
      false,
      'an active search/reveal target cannot hide its matched lines',
    );
    assert.equal(n.markdown, text);
    await f.root.act(() => original({ nativeEvent: { index: 0, generation } }));
    n = await f.render(props);
    assert.deepEqual([...n.quoteCollapse.expanded], [0, 1]);
    n = await f.render(props, true);
    assert.equal(n.quoteCollapse.enabled, false, 'selection reveals original full native text');
    assert.equal(n.selectable, true);
    assert.equal(n.markdown, text);
    await f.root.act(() => original({ nativeEvent: { index: 0, generation } }));
    n = await f.render(props);
    assert.deepEqual(
      [...n.quoteCollapse.expanded],
      [0, 1],
      'a late action cannot mutate selection presentation',
    );
    n = await f.render({ ...props, revision: 'r2' });
    assert.notEqual(n.quoteCollapse.generation, generation);
    assert.deepEqual(
      [...n.quoteCollapse.expanded],
      [],
      'even same text in a new revision starts fresh',
    );
    await f.root.act(() => original({ nativeEvent: { index: 0, generation } }));
    assert.deepEqual([...f.native.quoteCollapse.expanded], []);
    await f.render({ ...props, testID: 'message-body-B' });
    n = await f.render(props);
    assert.notEqual(
      n.quoteCollapse.generation,
      generation,
      'recycled ABA source cannot reuse an old generation',
    );
    await f.root.act(() => original({ nativeEvent: { index: 0, generation } }));
    assert.deepEqual([...f.native.quoteCollapse.expanded], []);
    const current = n.quoteCollapse.generation;
    for (const index of [-1, 0.5, NaN, 2147483648])
      await f.root.act(() => n.onBlockquoteToggle({ nativeEvent: { index, generation: current } }));
    assert.deepEqual([...f.native.quoteCollapse.expanded], []);
    n = await f.render({ ...props, text: 'literal source with no quote' });
    assert.equal(n.quoteCollapse.enabled, false);
    assert.equal(n.markdown, 'literal source with no quote');
    await f.root.close();
    await f.root.act(() => original({ nativeEvent: { index: 0, generation } }));
  } finally {
    await f.root.close();
  }
});

test('actual public GFM spec exposes typed quote presentation and its native toggle event', () => {
  const path = pkg + 'lib/module/EnrichedMarkdownNativeComponent.ts';
  const code = transformSync(readFileSync(path, 'utf8'), {
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
  }).code;
  const module = { exports: {} as any };
  let config: any;
  runInNewContext(code, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react-native/Libraries/NativeComponent/NativeComponentRegistry')
        return {
          get: (_name: string, read: () => unknown) => {
            config = read();
            return {};
          },
        };
      if (name === 'react-native/Libraries/NativeComponent/ViewConfigIgnore')
        return { ConditionallyIgnoredEventHandlers: (x: any) => x };
      if (name === 'react-native/Libraries/Components/View/ReactNativeStyleAttributes')
        return { colorAttribute: {} };
      if (name === 'react-native') return {};
      throw Error(name);
    },
  });
  assert.ok(config.validAttributes.quoteCollapse);
  assert.equal(config.directEventTypes.topBlockquoteToggle.registrationName, 'onBlockquoteToggle');
});

test('the actual message face passes its signed revision and only the active reveal expands quote content', () => {
  const message = plainMessage(text, { text_revision: 'signed-revision' });
  const ordinary = elements(renderFace(message).tree).find(
    (node) => node.type === 'MessageMarkdown',
  )!;
  assert.equal(ordinary.props.revision, 'signed-revision');
  assert.equal(ordinary.props.expandedForReveal, false);
  const revealAnchor = { id: message.id, onLayout() {}, onTextLayout() {} };
  const targeted = elements(renderFace(message, { revealAnchor }).tree).find(
    (node) => node.type === 'MessageMarkdown',
  )!;
  assert.equal(targeted.props.expandedForReveal, true);
  assert.equal(targeted.props.text, text);
  assert.equal(targeted.props.onLayout, revealAnchor.onLayout);
});
