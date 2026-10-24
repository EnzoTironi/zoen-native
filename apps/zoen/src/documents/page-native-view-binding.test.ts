import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const sourcePath = fileURLToPath(new URL('./page-input-view.ios.tsx', import.meta.url));
const compiled = transformSync(
  readFileSync(process.env.ZOEN_PAGE_INPUT_VIEW_SOURCE ?? sourcePath, 'utf8'),
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
assert.match(compiled, /react\/compiler-runtime/);

async function bundle(contents: string, loader: 'js' | 'tsx') {
  const output = await build({
    stdin: { contents, resolveDir: dirname(sourcePath), loader },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'native-view-boundary-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  });
  return output.outputFiles[0]!.text;
}

// Execute the installed SDK selector, caching, view-config processing and prop
// forwarding. The native registry below is the port; this is not UIKit proof.
const adapterSource = readFileSync(
  new URL(
    '../../node_modules/expo-modules-core/src/NativeViewManagerAdapter.native.tsx',
    import.meta.url,
  ),
  'utf8',
);
const adapterBundle = await bundle(adapterSource, 'tsx');
const wrapperBundle = await bundle(compiled, 'js');

type Element = { type: any; props: Record<string, any> };
function render(defaultView: string, props: Record<string, unknown>) {
  const registered: { name: string; config: Record<string, any> }[] = [];
  const adapter = { exports: {} as { requireNativeViewManager: (...args: any[]) => any } };
  const react = require('react');
  runInNewContext(adapterBundle, {
    module: adapter,
    exports: adapter.exports,
    expo: {
      getViewConfig(module: string, view?: string) {
        assert.equal(module, 'ZoenCore');
        return {
          validAttributes:
            (view ?? defaultView) === 'ZoenPageTextInput' ? { token: true } : { html: true },
          directEventTypes: {},
        };
      },
    },
    require(name: string) {
      if (name === 'react') return react;
      if (name === 'react/jsx-runtime') return require(name);
      if (name === './SharedObject') return { SharedObject: class {} };
      if (name === './requireNativeModule')
        return { requireNativeModule: () => ({ ViewPrototypes: {} }) };
      if (name === 'react-native')
        return {
          NativeComponentRegistry: {
            get(name: string, config: () => Record<string, any>) {
              registered.push({ name, config: config() });
              return name;
            },
          },
          findNodeHandle: () => null,
        };
      assert.fail(`Unexpected installed native-view adapter import: ${name}`);
    },
  });
  const wrapper = { exports: {} as { PageInputView: (props: unknown) => Element } };
  runInNewContext(wrapperBundle, {
    module: wrapper,
    exports: wrapper.exports,
    require(name: string) {
      if (name === 'expo') return { requireNativeView: adapter.exports.requireNativeViewManager };
      if (name === 'react/jsx-runtime') return require(name);
      if (name === 'react/compiler-runtime')
        return { c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')) };
      assert.fail(`Unexpected Page input wrapper import: ${name}`);
    },
  });
  const element = wrapper.exports.PageInputView(props);
  const native = new element.type(element.props).render() as Element;
  return { native, registered };
}

for (const defaultView of ['SitePreview', 'NavigationContainer'])
  test(`Page source editing retains its text producer when ${defaultView} is the default view`, () => {
    const style = { height: 330, width: '100%' };
    const { native, registered } = render(defaultView, {
      token: 'current-native-checkpoint',
      style,
      accessibilityLabel: 'Page Markdown',
    });
    assert.equal(registered.length, 1);
    assert.ok(
      Object.hasOwn(registered[0]!.config.validAttributes, 'token'),
      'the rendered native view must accept the checkpoint producer token',
    );
    assert.equal(Object.hasOwn(registered[0]!.config.validAttributes, 'html'), false);
    assert.equal(native.props.token, 'current-native-checkpoint');
    assert.equal(native.props.style, style);
    assert.equal(native.props.accessibilityLabel, 'Page Markdown');
  });
