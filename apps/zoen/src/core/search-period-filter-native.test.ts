import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');

test('production-compiled native date sheet uses picker events, exact reviewed bounds and retires old controls', async () => {
  const code = await build({
    entryPoints: ['src/components/search-period-filter.ios.tsx'],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'native-calendar-ports',
        setup(builder) {
          builder.onResolve(
            {
              filter:
                /^(react|react\/jsx-runtime|react\/compiler-runtime|expo-router|react-native|@expo\/ui(?:\/.*)?|\.\.\/i18n|\.\.\/theme|\.\/primitives)$/,
            },
            ({ path }) => ({ path, external: true }),
          );
          builder.onLoad({ filter: /search-period-filter\.ios\.tsx$/ }, ({ path }) => ({
            contents: transformSync(readFileSync(path, 'utf8'), {
              filename: path,
              configFile: false,
              babelrc: false,
              presets: ['babel-preset-expo'],
              plugins: [['babel-plugin-react-compiler', { target: '19' }]],
              caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
            }).code,
            loader: 'js',
          }));
        },
      },
    ],
  });
  const root = createReactTestRoot();
  const buttons = new Map<string, any>(),
    pickers = new Map<string, any>();
  const values: unknown[] = [];
  let sheet: any,
    current = true;
  const lifecycle: { listener: ((state: string) => void) | null } = { listener: null };
  const app = {
    currentState: 'active',
    addEventListener: (_name: string, listener: (state: string) => void) => {
      lifecycle.listener = listener;
      return {
        remove: () => {
          lifecycle.listener = null;
        },
      };
    },
  };
  const Container = ({ children }: any) => React.createElement(React.Fragment, null, children);
  const module = { exports: {} as any };
  runInNewContext(code.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'expo-router')
        return {
          useFocusEffect: (callback: () => void | (() => void)) =>
            React.useEffect(callback, [callback]),
        };
      if (name === 'react-native')
        return { AppState: app, View: Container, ScrollView: Container, Text: () => null };
      if (name === '@expo/ui')
        return {
          Host: Container,
          RNHostView: Container,
          BottomSheet: (props: any) => {
            sheet = props;
            return props.isPresented ? React.createElement(Container, props) : null;
          },
        };
      if (name === '@expo/ui/community/datetime-picker')
        return {
          __esModule: true,
          default: (props: any) => {
            pickers.set(props.testID, props);
            return null;
          },
        };
      if (name === '../theme') return { useTheme: () => ({ colors: {}, scheme: 'dark' }) };
      if (name === '../i18n') return { t: (key: string) => key, formatLocale: 'en-US' };
      if (name === './primitives')
        return {
          NativeButton: (props: any) => {
            buttons.set(props.testID, props);
            return null;
          },
          IconButton: () => null,
        };
      return require(name);
    },
  });
  try {
    await root.render(
      React.createElement(module.exports.SearchPeriodFilter, {
        selected: false,
        isCurrent: () => current,
        onChange: (value: unknown) => values.push(value),
      }),
    );
    assert.equal(values.length, 0);
    assert.equal(sheet.isPresented, false);
    await root.act(() => buttons.get('search-custom-period').onPress());
    const staleDismiss = sheet.onDismiss;
    const firstStart = pickers.get('search-period-start');
    await root.act(() => {
      firstStart.onValueChange({}, new Date(2026, 0, 2));
      firstStart.onValueChange({}, new Date(2026, 0, 3));
      pickers.get('search-period-end').onValueChange({}, new Date(2026, 0, 1));
    });
    assert.equal(buttons.get('search-period-apply').disabled, true);
    const staleApply = buttons.get('search-period-apply').onPress;
    await root.act(() => pickers.get('search-period-end').onValueChange({}, new Date(2026, 0, 8)));
    await root.act(staleApply);
    assert.equal(values.length, 0);
    await root.act(() => buttons.get('search-period-apply').onPress());
    assert.deepEqual(JSON.parse(JSON.stringify(values)), [
      { start_ms: new Date(2026, 0, 3).getTime(), end_ms: new Date(2026, 0, 9).getTime() },
    ]);
    await root.act(() => buttons.get('search-custom-period').onPress());
    await root.act(staleDismiss);
    assert.equal(sheet.isPresented, true);
    current = false;
    await root.act(() => buttons.get('search-period-apply').onPress());
    assert.equal(values.length, 1);
    app.currentState = 'background';
    const background = lifecycle.listener;
    if (!background) throw Error('missing app lifecycle subscription');
    await root.act(() => background('background'));
    assert.equal(sheet.isPresented, false);
    const retained = buttons.get('search-custom-period').onPress;
    await root.close();
    current = true;
    app.currentState = 'active';
    retained();
    assert.equal(values.length, 1);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
