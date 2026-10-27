import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
const require = createRequire(import.meta.url);
const file = new URL('../components/primitives.tsx', import.meta.url);
const { code } = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  configFile: false,
  babelrc: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
});
const module = { exports: {} as any };
const jsx = (type: unknown, props: unknown) => ({ type, props });
runInNewContext(code, {
  module,
  exports: module.exports,
  require(name: string) {
    if (name.startsWith('@babel/runtime/')) return require(name);
    if (name === 'react') return React;
    if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
    if (name === './native-button-label') return { FullWidthButtonLabel: 'FullWidthButtonLabel' };
    if (name === '@expo/ui') return { Host: 'Host', Button: 'Button', Text: 'NativeText' };
    if (name === 'expo-symbols') return { SymbolView: 'SymbolView' };
    if (name === 'expo-image') return { Image: 'Image' };
    if (name === 'react-native') return {};
    if (name === '../theme')
      return { useTheme: () => ({ scheme: 'dark', presentation: { accent: '#08f' } }) };
    if (name === '../theme/telegram') return {};
    if (name === '../i18n/index.ts') return { t: (value: string) => value };
    throw Error(name);
  },
});
// The native screenshot is the overflow reproduction. This fixture checks the
// real component's SwiftUI layout boundary, not native text measurements.
test('long discussion labels keep finite parent width and grow vertically without dropping the label or action', () => {
  let presses = 0;
  const label = 'Comentar na conversa';
  const tree = module.exports.NativeButton({
    label,
    wrapLabel: true,
    onPress: () => presses++,
    secondary: true,
    disabled: true,
  });
  assert.equal(tree.type, 'Host');
  assert.equal(tree.props.matchContents.horizontal, undefined);
  assert.equal(tree.props.matchContents.vertical, true);
  assert.equal(tree.props.style.alignSelf, 'stretch');
  const button = tree.props.children;
  assert.equal(button.type, 'Button');
  assert.equal(button.props.style.width, '100%');
  assert.equal(button.props.label, label);
  assert.equal(button.props.disabled, true);
  assert.equal(button.props.variant, 'outlined');
  assert.equal(button.props.children.type, 'NativeText');
  assert.equal(button.props.children.props.children, label);
  assert.equal(button.props.children.props.numberOfLines, undefined);
  button.props.onPress();
  assert.equal(presses, 1);
});
test('existing compact and header buttons retain intrinsic sizing and plain native labels', () => {
  const tree = module.exports.NativeButton({ label: 'Histórico', onPress() {} });
  assert.equal(tree.props.matchContents, true);
  assert.equal(tree.props.style, undefined);
  assert.equal(tree.props.children.props.style, undefined);
  assert.equal(tree.props.children.props.label, 'Histórico');
  assert.equal(tree.props.children.props.children, null);
});
