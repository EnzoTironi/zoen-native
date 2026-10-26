import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { fixture, settle } from './community-members.fixtures.ts';
import { telegramPresentation } from '../theme/telegram.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const source = new URL('../communities/member-actions-sheet.tsx', import.meta.url);
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
type Menu = {
  actions: { id: string; title: string; attributes: { disabled: boolean } }[];
  onPressAction(event: { nativeEvent: { event: string } }): void;
  children?: React.ReactNode;
};

/** Real React/Compiler + real external-store session. Native controls are ports;
 * invoking a captured callback does not prove native hit testing or touch delivery. */
export function memberReactFixture() {
  const f = fixture();
  const root = createReactTestRoot();
  const controls = new Map<string, Action>();
  const routes: unknown[] = [],
    confirmations: unknown[] = [];
  let sheet: Sheet | null = null;
  let menu: Menu | null = null;
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
  const CapturedRow = ({ row }: { row: Action & { id: string } }) => useCapture(row.id, row);
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
    '@expo/ui/community/menu': {
      MenuView: (props: Menu) => {
        React.useLayoutEffect(() => {
          menu = props;
          return () => {
            if (menu === props) menu = null;
          };
        }, [props]);
        return children(props);
      },
    },
    'react-native': { ScrollView: children, View: children, Text: () => null },
    'expo-router': { router: { push: (route: unknown) => routes.push(structuredClone(route)) } },
    '../i18n': { t: (key: string) => key },
    '../theme': { useTheme: () => theme },
    '../components/dialogs': { useDialogs: () => ({ confirm }) },
    '../components/primitives': {
      Icon: () => null,
      Loading: () => null,
      IconButton: (props: Action & { label: string }) => useCapture(props.label, props),
      NativeButton: (props: Action & { testID?: string; label: string }) =>
        useCapture(props.testID ?? props.label, props),
    },
    '../components/settings-rows': {
      SettingsRows: ({ rows }: { rows: Array<Action & { id: string }> }) =>
        React.createElement(
          React.Fragment,
          null,
          rows.map((row) => React.createElement(CapturedRow, { key: row.id, row })),
        ),
    },
    '../profile/profile-avatar': { ProfileAvatar: () => null },
  };
  const module = { exports: {} as { MemberActionsSheet: React.ComponentType<any> } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
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
    return React.createElement(module.exports.MemberActionsSheet, { session: f.session, state });
  }
  return {
    ...f,
    routes,
    confirmations,
    render: () => root.render(React.createElement(Surface)),
    act: root.act,
    errors: root.errors,
    close: root.close,
    async open(id?: string) {
      await root.act(() => f.open(id));
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
    menu: () => {
      assert.ok(menu);
      return menu;
    },
    confirm(next: typeof answer) {
      answer = next;
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
