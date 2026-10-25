import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { additionFixture, settle } from './member-addition.fixtures.ts';
import { telegramPresentation } from '../theme/telegram.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const source = new URL('../communities/member-addition-sheet.tsx', import.meta.url);
const compiled = transformSync(readFileSync(source, 'utf8'), {
  filename: source.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;

type Action = { onPress(): void; disabled?: boolean };
type Sheet = { isPresented: boolean; onDismiss(): void; children?: React.ReactNode };

/** Real React/Compiler + real external-store session. Native controls are ports;
 * invoking a captured callback does not prove native hit testing or touch delivery. */
export async function memberAdditionReactFixture(inline = false) {
  const f = await additionFixture();
  const root = createReactTestRoot();
  const controls = new Map<string, Action>();
  const routes: unknown[] = [],
    confirmations: unknown[] = [];
  let sheet: Sheet | null = null;
  const texts = new Map<object, string>();
  let answer: (options: unknown) => Promise<boolean> = async () => true;
  const useCapture = (id: string, props: Action) => {
    React.useLayoutEffect(() => {
      controls.set(id, props);
      return () => {
        if (controls.get(id) === props) controls.delete(id);
      };
    }, [id, props]);
    return null;
  };
  const children = ({ children }: { children?: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children);
  const confirm = (options: unknown) => {
    confirmations.push(options);
    return answer(options);
  };
  const theme = { presentation: telegramPresentation('day'), scheme: 'light' };
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    '@expo/ui': {
      Host: children,
      RNHostView: children,
      BottomSheet: (props: Sheet) => {
        React.useLayoutEffect(() => {
          sheet = props;
          return () => {
            if (sheet === props) sheet = null;
          };
        }, [props]);
        return props.isPresented ? children(props) : null;
      },
    },
    'react-native': {
      View: children,
      Text: ({ children }: { children?: React.ReactNode }) => {
        const token = React.useRef({}).current;
        const text = React.Children.toArray(children)
          .filter((child) => typeof child === 'string')
          .join(' ');
        React.useLayoutEffect(() => {
          texts.set(token, text);
          return () => {
            texts.delete(token);
          };
        }, [token, text]);
        return null;
      },
      Pressable: (props: Action & { testID: string; children?: React.ReactNode }) => {
        useCapture(props.testID, props);
        return children(props);
      },
      FlatList: (props: {
        data: unknown[];
        renderItem: (item: { item: unknown }) => React.ReactNode;
      }) =>
        React.createElement(
          React.Fragment,
          null,
          props.data.map((item, key) =>
            React.createElement(React.Fragment, { key }, props.renderItem({ item })),
          ),
        ),
    },
    'expo-router': { router: { push: (route: unknown) => routes.push(structuredClone(route)) } },
    '../i18n': { t: (key: string) => key },
    '../theme': { useTheme: () => theme },
    '../components/dialogs': { useDialogs: () => ({ confirm }) },
    '../components/primitives': {
      Loading: () => null,
      Field: (props: any) => useCapture('search', props),
      IconButton: (props: Action & { label: string }) => useCapture(props.label, props),
      NativeButton: (props: Action & { testID?: string; label: string }) =>
        useCapture(props.testID ?? props.label, props),
    },
    '../profile/profile-avatar': { ProfileAvatar: () => null },
  };
  const module = {
    exports: {} as {
      MemberAdditionSheet: React.ComponentType<any>;
      MemberAdditionContent: React.ComponentType<any>;
    },
  };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  function Surface() {
    const state = React.useSyncExternalStore(
      f.session.subscribe,
      f.session.getSnapshot,
      f.session.getSnapshot,
    );
    return React.createElement(
      inline ? module.exports.MemberAdditionContent : module.exports.MemberAdditionSheet,
      {
        session: f.session,
        state,
        inlineConfirmation: inline,
        onBack: inline ? () => routes.push('back') : undefined,
      },
    );
  }
  return {
    ...f,
    routes,
    confirmations,
    text: () => [...texts.values()].join(' '),
    render: () => root.render(React.createElement(Surface)),
    act: root.act,
    errors: root.errors,
    close: root.close,
    async open() {
      await root.act(() => f.open());
    },
    control(id: string) {
      const value = controls.get(id);
      assert.ok(value, id);
      return value;
    },
    sheet: () => {
      assert.ok(sheet);
      return sheet;
    },
    confirm(next: typeof answer) {
      answer = next;
    },
    async search(query: string) {
      await root.act(async () => {
        const field = controls.get('search') as unknown as { onChangeText(value: string): void };
        assert.ok(field);
        field.onChangeText(query);
        await f.session.refresh();
      });
    },
    async press(id: string) {
      await root.act(async () => {
        const action = controls.get(id);
        assert.ok(action, id);
        action.onPress();
        await settle();
      });
    },
  };
}
