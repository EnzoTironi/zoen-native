import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { deferred } from './store.fixtures.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const source = fileURLToPath(new URL('../components/message-markdown.tsx', import.meta.url));
const ports = new Set([
  'react',
  'react/jsx-runtime',
  'react/compiler-runtime',
  'react-native',
  '../EnrichedMarkdownNativeComponent',
  '../EnrichedMarkdownTextNativeComponent',
  'expo-clipboard',
  'expo-haptics',
  '../theme',
  '../i18n/index.ts',
  './dialogs',
  './use-link-launch',
]);
const compiled = await build({
  entryPoints: [source],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'compiled-component',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (path === 'react-native-enriched-markdown')
            return { path, namespace: 'native-adapter' };
          return ports.has(path) ? { path, external: true } : undefined;
        });
        builder.onLoad({ filter: /.*/, namespace: 'native-adapter' }, () => ({
          contents: `export { default as EnrichedMarkdownText } from ${JSON.stringify(fileURLToPath(new URL('../../node_modules/react-native-enriched-markdown/src/native/EnrichedMarkdownText.tsx', import.meta.url)))};`,
          loader: 'tsx',
          resolveDir: fileURLToPath(new URL('.', import.meta.url)),
        }));
        builder.onLoad(
          { filter: /(?:message-markdown\.tsx|use-message-code-copy\.ts)$/ },
          ({ path }) => ({
            contents: transformSync(readFileSync(path, 'utf8'), {
              filename: path,
              babelrc: false,
              configFile: false,
              presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
              plugins: [['babel-plugin-react-compiler', { target: '19' }]],
              caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
            }).code,
            loader: 'js',
          }),
        );
      },
    },
  ],
});

test('compiled message copies native code and retires old callbacks and feedback', async () => {
  const root = createReactTestRoot(),
    copies: string[] = [],
    notices: string[] = [];
  let haptics = 0;
  const hapticPending = deferred<void>();
  let result: Promise<boolean> = Promise.resolve(true),
    props: any;
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
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
          Platform: { OS: 'ios', select: (options: Record<string, unknown>) => options.ios },
          processColor: (color: unknown) => color,
        };
      if (
        name === '../EnrichedMarkdownNativeComponent' ||
        name === '../EnrichedMarkdownTextNativeComponent'
      )
        return {
          __esModule: true,
          default: (p: any) => {
            props = p;
            return null;
          },
        };
      if (name === 'expo-clipboard')
        return {
          setStringAsync: (code: string) => {
            copies.push(code);
            return result;
          },
        };
      if (name === 'expo-haptics')
        return {
          selectionAsync: () => {
            haptics++;
            return hapticPending.promise;
          },
        };
      if (name === '../theme') return { useTheme: () => ({ colors: {}, scheme: 'light' }) };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === './dialogs')
        return {
          useDialogs: () => ({
            inform: async (title: string, message: string) => {
              assert.equal(message, '');
              notices.push(title);
            },
          }),
        };
      if (name === './use-link-launch') return { useLinkLaunch: () => () => {} };
      throw Error(name);
    },
  });
  const render = (text: string, identity = 'message-body-a') =>
    root.render(
      React.createElement(module.exports.MessageMarkdown, {
        text,
        testID: identity,
        color: '#000',
        fontSize: 16,
        maxWidth: 300,
      }),
    );
  try {
    await render('```\nA\n```');
    assert.equal(props.enableCodeBlockPress, true);
    const old = props.onCodeBlockPress;
    await root.act(async () => {
      old({ nativeEvent: { code: '  A\n', language: 'js' } });
      await result;
    });
    assert.deepEqual(copies, ['  A\n']);
    assert.deepEqual(notices, []);
    assert.equal(haptics, 1); // The haptic promise remains pending without opening a modal.
    result = Promise.resolve(false);
    await root.act(async () => {
      props.onCodeBlockPress({ nativeEvent: { code: 'B', language: '' } });
      await result;
    });
    assert.equal(notices.at(-1), 'chat.copyCodeFailed');
    result = Promise.reject(new Error('clipboard unavailable'));
    await root.act(async () => {
      props.onCodeBlockPress({ nativeEvent: { code: '', language: '' } });
      await result.catch(() => {});
    });
    assert.equal(notices.at(-1), 'chat.copyCodeFailed');
    const pending = deferred<boolean>();
    result = pending.promise;
    await root.act(() =>
      props.onCodeBlockPress({ nativeEvent: { code: 'pending', language: '' } }),
    );
    await render('```\nB\n```');
    await render('```\nA\n```');
    await root.act(async () => {
      pending.resolve(false);
      await pending.promise;
    });
    assert.equal(notices.length, 2);
    assert.equal(haptics, 1);
    await root.act(() => old({ nativeEvent: { code: 'stale', language: '' } }));
    assert.deepEqual(copies, ['  A\n', 'B', '', 'pending']);
    const previousIdentity = props.onCodeBlockPress;
    await render('```\nA\n```', 'message-body-b');
    await root.act(() =>
      previousIdentity({ nativeEvent: { code: 'retired identity', language: '' } }),
    );
    assert.equal(copies.length, 4);
    const unmountPending = deferred<boolean>();
    result = unmountPending.promise;
    const beforeUnmount = props.onCodeBlockPress;
    await root.act(() => beforeUnmount({ nativeEvent: { code: 'unmount pending', language: '' } }));
    await root.render(null);
    await root.act(async () => {
      unmountPending.resolve(true);
      await unmountPending.promise;
    });
    await root.act(() => beforeUnmount({ nativeEvent: { code: 'unmounted', language: '' } }));
    assert.equal(copies.length, 5);
    assert.equal(notices.length, 2);
    assert.equal(haptics, 1);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
