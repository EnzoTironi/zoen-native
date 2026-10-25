import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

const require = createRequire(import.meta.url);
type Props = Record<string, any>;

/** Real compiled route and React state. Native header/keyboard are ports;
 * visibility above the software keyboard still needs the iPhone journey. */
export async function newChatFixture(platform = 'ios', members: Props[] = []) {
  const root = createReactTestRoot();
  const controls = new Map<string, Props>();
  const calls: any[] = [],
    routes: any[] = [];
  let resolve!: (id: string) => void, reject!: (error: Error) => void;
  const request = (command: unknown) => {
    calls.push(structuredClone(command));
    return new Promise<string>((yes, no) => {
      resolve = yes;
      reject = no;
    });
  };
  function children(props: Props) {
    return props.children;
  }
  function capture(key: (props: Props) => string, showChildren = false) {
    return function Capture(props: Props) {
      React.useLayoutEffect(() => {
        const name = key(props);
        controls.set(name, props);
        return () => {
          if (controls.get(name) === props) controls.delete(name);
        };
      }, [props]);
      return showChildren ? props.children : null;
    };
  }
  const Toolbar = Object.assign(
    capture(() => 'toolbar', true),
    {
      Button: capture(() => 'createHeader'),
    },
  );
  const file = new URL('../app/new-chat.tsx', import.meta.url);
  const { code } = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    configFile: false,
    babelrc: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform, isDev: false, supportsStaticESM: false },
  });
  assert.match(code, /react\/compiler-runtime/);
  const loaded = { exports: {} as { default: React.ComponentType } };
  runInNewContext(code, {
    module: loaded,
    exports: loaded.exports,
    Error,
    setTimeout,
    clearTimeout,
    require(name: string) {
      if (name === 'react') return React;
      if (
        name === 'react/jsx-runtime' ||
        name === 'react/compiler-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'expo-router')
        return {
          Stack: { Toolbar },
          router: {
            dismiss: () => routes.push('dismiss'),
            push: (route: unknown) => routes.push(structuredClone(route)),
          },
        };
      if (name === 'react-native')
        return {
          Platform: { OS: platform },
          View: children,
          Text: capture((props) => `text:${props.children}`),
          ScrollView: capture(() => 'scroll', true),
          Pressable: capture(() => 'person'),
        };
      if (name === '../core/provider')
        return {
          useClient: () => ({
            store: { request, driver: { request } },
            snapshot: { conversations: [{ members }] },
          }),
        };
      if (name === '../theme')
        return {
          useTheme: () => ({
            colors: { text: '#fff', secondary: '#aaa', accent: '#0af', danger: '#f00' },
          }),
        };
      if (name === '../profile/profile-avatar') return { ProfileAvatar: () => null };
      if (name === '../i18n/index.ts') return { t: (key: string) => key, formatLocale: 'en-US' };
      if (name === '../components/primitives')
        return {
          NativeButton: capture((props) => `button:${props.label}`),
          Field: capture((props) => `field:${props.label}`),
        };
      throw Error(`Unexpected new-chat import ${name}`);
    },
  });
  const control = (name: string) => {
    const value = controls.get(name);
    if (!value) throw Error(`Missing new-chat control ${name}`);
    return value;
  };
  await root.render(React.createElement(loaded.exports.default));
  return {
    controls,
    calls,
    routes,
    control,
    errors: root.errors,
    act: root.act,
    close: root.close,
    choose: (kind: string) => root.act(() => control(`button:chat.${kind}`).onPress()),
    title: (value: string, kind = 'group') =>
      root.act(() => control(`field:chat.${kind}Name`).onChangeText(value)),
    resolve: (id: string) => root.act(() => resolve(id)),
    reject: (message: string) => root.act(() => reject(Error(message))),
  };
}
