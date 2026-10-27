import assert from 'node:assert/strict';
import type { EnrichedMarkdownTextInputInstance } from 'react-native-enriched-markdown';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as content from '../media/attachment-content.ts';
import { editorCode, hooks, nodes, type Element } from './attachment-route-ui.fixtures.ts';
const require = createRequire(import.meta.url);
const nativeCode = await build({
  entryPoints: [
    fileURLToPath(
      new URL(
        '../../node_modules/react-native-enriched-markdown/src/EnrichedMarkdownTextInput.tsx',
        import.meta.url,
      ),
    ),
  ],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'actual-enrm-fabric-ports',
      setup(builder) {
        builder.onResolve({ filter: /EnrichedMarkdownTextInputNativeComponent$/ }, () => ({
          path: 'FabricMarkdown',
          external: true,
        }));
        builder.onResolve(
          { filter: /^(react|react\/jsx-runtime|react-native(?:\/.*)?)$/ },
          ({ path }) => ({ path, external: true }),
        );
      },
    },
  ],
});
const adapterCode = require('@babel/core').transformSync(
  readFileSync(new URL('../media/article-native-input.tsx', import.meta.url), 'utf8'),
  {
    filename: 'article-native-input.tsx',
    configFile: false,
    babelrc: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  },
).code;
const tick = async () => {
  for (let i = 0; i < 40; i++) await Promise.resolve();
};
export function nativeEditorFixture() {
  const parent = hooks();
  let child = hooks(),
    adapterHooks = hooks();
  const calls: unknown[][] = [];
  const input = { current: null as any };
  const host = { measure() {}, measureInWindow() {}, measureLayout() {} };
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  let publicInputRef: { current: EnrichedMarkdownTextInputInstance | null };
  let leaf: Element,
    tree: Element,
    generation = 0,
    enabled = true;
  const commands = new Proxy(
    {},
    {
      get:
        (_, name) =>
        (...args: unknown[]) =>
          calls.push([name, ...args]),
    },
  );
  const focusState = {
    registerInput() {},
    unregisterInput() {},
    currentlyFocusedInput: () => null,
    blurTextInput() {},
    focusInput() {},
    blurInput() {},
    focusTextInput() {},
  };
  const adapter = { exports: {} as any };
  runInNewContext(adapterCode, {
    module: adapter,
    exports: adapter.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react')
        return {
          useRef: (...args: any[]) => adapterHooks.react.useRef(args[0]),
          useImperativeHandle: (ref: any, read: () => unknown, deps: unknown[]) =>
            adapterHooks.react.useImperativeHandle(ref, read, deps),
        };
      if (name === 'react/compiler-runtime')
        return { c: (size: number) => adapterHooks.runtime.c(size) };
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownTextInput: 'ActualENRM' };
      throw Error(name);
    },
  });
  const actual = { exports: {} as any };
  runInNewContext(nativeCode.outputFiles[0]!.text, {
    module: actual,
    exports: actual.exports,
    require: (name: string) => {
      if (name === 'react')
        return Object.fromEntries(
          Object.keys(child.react).map((key) => [
            key,
            (...args: any[]) => (child.react as any)[key](...args),
          ]),
        );
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native')
        return { Platform: { OS: 'ios' }, processColor: (value: unknown) => value };
      if (name === 'FabricMarkdown') return { default: 'FabricMarkdown', Commands: commands };
      if (name === 'react-native/Libraries/Components/TextInput/TextInputState') return focusState;
      if (name === 'react-native/Libraries/Pressability/usePressability') return () => ({});
      throw Error(name);
    },
  });
  const article = { exports: {} as any };
  runInNewContext(editorCode, {
    module: article,
    exports: article.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return parent.react;
      if (name === 'react/compiler-runtime') return parent.runtime;
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native')
        return { View: 'View', Text: 'Text', ScrollView: 'ScrollView', TextInput: 'TextInput' };
      if (name === './article-native-input') return { ArticleNativeInput: 'ArticleNativeInput' };
      if (name === '../components/primitives')
        return { Field: 'Field', NativeButton: 'NativeButton', IconButton: 'IconButton' };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === './attachment-content') return content;
      throw Error(name);
    },
  });
  function render() {
    parent.begin();
    tree = article.exports.ArticleEditor({
      initial: 'before selected after',
      disabled: false,
      source: false,
      onChange: () => calls.push(['changed']),
      canEdit: () => enabled,
      captureEditLease: () => {
        const g = generation;
        return () => enabled && g === generation;
      },
      ref: input,
    });
    const editor = nodes(tree).find((n) => n.type === 'ArticleNativeInput');
    assert.ok(editor);
    adapterHooks.begin();
    const native = adapter.exports.ArticleNativeInput(editor.props);
    publicInputRef = native.props.ref;
    child.begin();
    leaf = actual.exports.EnrichedMarkdownTextInput(native.props);
    leaf.props.ref.current = host;
    child.commit();
    adapterHooks.commit(); // React commits the child's imperative handle before parent layout effects.
    parent.commit();
    return tree;
  }
  const find = (label: string) => nodes(tree).find((n) => n.props.label === label);
  const reply = async (markdown = 'before selected after') => {
    const request = calls.filter((call) => call[0] === 'requestMarkdown').at(-1);
    assert.ok(request);
    leaf.props.onRequestMarkdownResult({ nativeEvent: { requestId: request[2], markdown } });
    await tick();
    return render();
  };
  return {
    render,
    find,
    calls,
    reply,
    leaf: () => leaf,
    select: (start: number, end: number) =>
      leaf.props.onChangeSelection({ nativeEvent: { start, end } }),
    retire: () => {
      generation++;
      enabled = false;
    },
    resume: () => {
      enabled = true;
    },
    unmount: () => {
      child.retire();
      adapterHooks.retire();
      parent.retire();
    },
    nativeHandle: () => publicInputRef.current,
    replaceInput: () => {
      child.retire();
      adapterHooks.retire();
      child = hooks();
      adapterHooks = hooks();
      render();
    },
  };
}
