import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

const source = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const ui = source('../../node_modules/@expo/ui/src/');
const ports = new Set([
  'react',
  'react/jsx-runtime',
  'react/compiler-runtime',
  'react-native',
  '@expo/ui/swift-ui',
  'expo',
  'expo-symbols',
  'expo-image',
  '../theme',
  '../theme/telegram',
  '../i18n/index.ts',
]);
const compiled = await build({
  entryPoints: [source('../components/primitives.tsx')],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'installed-ios-button-adapters',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path, importer }) => {
          if (ports.has(path)) return { path, external: true };
          if (path === '@expo/ui') return { path, namespace: 'installed-ui' };
          if (path === './native-button-label')
            return { path: source('../components/native-button-label.ios.tsx') };
          if (path === '@expo/ui/swift-ui/modifiers')
            return { path: ui + 'swift-ui/modifiers/index.ts' };
          if (path === '../transformStyle' && importer.includes('/universal/'))
            return { path: ui + 'universal/transformStyle.ios.ts' };
          return undefined;
        });
        builder.onLoad({ filter: /.*/, namespace: 'installed-ui' }, () => ({
          contents: `export { Host } from '@expo/ui/swift-ui'; export { Button } from ${JSON.stringify(ui + 'universal/Button/index.ios.tsx')}; export { Text } from ${JSON.stringify(ui + 'universal/Text/index.ios.tsx')};`,
          loader: 'tsx',
          resolveDir: ui,
        }));
      },
    },
  ],
});

test('full-width action sizes the actual SwiftUI label inside the styled Button, without fixing its height', async () => {
  const root = createReactTestRoot();
  const buttons: Record<string, any>[] = [],
    texts: Record<string, any>[] = [];
  let presses = 0;
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    __DEV__: false,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/jsx-runtime') return jsxRuntime;
      if (name === 'react/compiler-runtime') throw Error('unexpected compiler port');
      if (name === '@expo/ui/swift-ui')
        return {
          Host: ({ children }: any) => React.createElement(React.Fragment, null, children),
          Button: (props: any) => {
            buttons.push(props);
            return React.createElement(React.Fragment, null, props.children);
          },
          Text: (props: any) => {
            texts.push(props);
            return null;
          },
        };
      if (name === 'expo') return { requireNativeModule: () => ({}) };
      if (name === 'react-native') return {};
      if (name === 'expo-symbols') return { SymbolView: () => null };
      if (name === 'expo-image') return { Image: () => null };
      if (name === '../theme')
        return { useTheme: () => ({ scheme: 'dark', presentation: { accent: '#08f' } }) };
      if (name === '../theme/telegram') return {};
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      throw Error(name);
    },
  });
  try {
    const label = 'Encaminhar novamente para esta conversa';
    await root.render(
      React.createElement(module.exports.NativeButton, {
        label,
        fullWidth: true,
        onPress: () => presses++,
      }),
    );
    const button = buttons.at(-1)!;
    assert.equal(button.label, undefined, 'native button must use custom label content');
    assert.ok(
      button.modifiers.some(
        (value: any) => value.$type === 'buttonStyle' && value.style === 'borderedProminent',
      ),
    );
    assert.equal(texts.at(-1)!.children, label);
    const frame = texts.at(-1)!.modifiers.find((value: any) => value.$type === 'frame');
    assert.equal(frame.maxWidth, Infinity);
    assert.equal(frame.minHeight, 44);
    assert.equal(frame.height, undefined);
    assert.equal(frame.maxHeight, undefined);
    assert.equal(
      texts.at(-1)!.modifiers.some((value: any) => value.$type === 'lineLimit'),
      false,
    );
    button.onPress();
    assert.equal(presses, 1);
    await root.render(
      React.createElement(module.exports.NativeButton, {
        label: 'Cancelar',
        fullWidth: true,
        secondary: true,
        disabled: true,
        onPress: () => presses++,
      }),
    );
    assert.ok(
      buttons
        .at(-1)!
        .modifiers.some((value: any) => value.$type === 'disabled' && value.disabled === true),
    );
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
