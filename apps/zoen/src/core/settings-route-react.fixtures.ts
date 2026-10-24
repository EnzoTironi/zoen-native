import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver } from './store.fixtures.ts';
import { mutedSnapshot } from './muted-chats.fixtures.ts';
import * as devices from '../devices/session.ts';
import * as blocked from '../profile/blocked-people.ts';
import * as muted from '../notifications/muted-chats.ts';
import * as ownership from './account-scope.ts';
import { telegramPresentation } from '../theme/telegram.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
export type SettingsKind = 'devices' | 'blocked' | 'muted';
const paths = {
  devices: [
    '../app/devices.tsx',
    '../devices/use-devices.ts',
    '../devices/device-list.tsx',
    '../devices/device-details.tsx',
  ],
  blocked: [
    '../app/settings/blocked-people.tsx',
    '../profile/use-blocked-people.ts',
    '../profile/blocked-people-view.tsx',
  ],
  muted: [
    '../app/settings/muted-chats.tsx',
    '../notifications/use-muted-chats.ts',
    '../notifications/muted-chats-screen.tsx',
  ],
};
const compiled = Object.fromEntries(
  Object.entries(paths).map(([kind, files]) => [
    kind,
    files.map((path) => {
      const file = new URL(path, import.meta.url);
      return transformSync(readFileSync(file, 'utf8'), {
        filename: file.pathname,
        babelrc: false,
        configFile: false,
        presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
        plugins: [['babel-plugin-react-compiler', { target: '19' }]],
        caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
      }).code as string;
    }),
  ]),
) as Record<SettingsKind, string[]>;

type Control = { onPress(): void; disabled?: boolean; isPresented?: boolean };
export async function settingsReactFixture(kind: SettingsKind) {
  const root = createReactTestRoot();
  const snapshot = mutedSnapshot(),
    driver = new Driver();
  const self = snapshot.account!.device_id,
    other = 'c'.repeat(64),
    person = 'd'.repeat(64);
  let directory = [
    { device_id: self, this_device: true, revoked: false },
    { device_id: other, this_device: false, revoked: false },
  ];
  let people = [person];
  const calls: any[] = [],
    routes: unknown[] = [],
    dialogs: unknown[] = [];
  let confirm: (options: unknown) => Promise<boolean> = async () => true;
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (op) => {
    calls.push(op);
    if (op.op === 'snapshot') return structuredClone(snapshot);
    if (op.op === 'devices') return structuredClone(directory);
    if (op.op === 'unlink_device') {
      directory = directory.filter((d) => d.device_id !== op.device_id);
      return;
    }
    if (op.op === 'blocked_people') return [...people];
    if (op.op === 'profile')
      return {
        identity_id: person,
        name: 'Synthetic person',
        handle: 'person',
        is_me: false,
        photo_ready: false,
      };
    if (op.op === 'unblock_person') {
      people = people.filter((id) => id !== op.identity_id);
      return;
    }
    if (op.op === 'set_chat_muted') {
      snapshot.conversations = snapshot.conversations.map((c) =>
        c.id === op.space_id ? { ...c, muted: false, muted_until_ms: null } : c,
      );
      return;
    }
    throw Error(`Unexpected Core operation ${op.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  let foreground = 'active',
    focused = true;
  const appListeners = new Set<(value: string) => void>();
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', callback: () => void) {
      focusListeners[name].add(callback);
      return () => focusListeners[name].delete(callback);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as { useFocusEffect: unknown } };
  runInNewContext(
    readFileSync(require.resolve('expo-router/build/link/useLoadedNavigation.js'), 'utf8'),
    {
      module: loaded,
      exports: loaded.exports,
      require: (name: string) => {
        if (name === 'react') return React;
        if (name === '../react-navigation/native')
          return {
            useNavigation: () => navigation,
            NavigationContainerRefContext: React.createContext({}),
          };
        throw Error(name);
      },
    },
  );
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    require: (name: string) => {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const controls = new Map<string, Control>();
  const children = ({ children }: { children?: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children);
  const Capture = (props: any) => {
    const key = props.testID ?? props.label;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (controls.get(key) === props) controls.delete(key);
      };
    }, [props, key]);
    return children(props);
  };
  const settingsRows = (props: any) =>
    React.createElement(
      React.Fragment,
      null,
      props.rows.map((row: any) =>
        React.createElement(Capture, { ...row, key: row.id, testID: row.id }),
      ),
    );
  const flatList = (props: any) =>
    React.createElement(
      React.Fragment,
      null,
      props.ListHeaderComponent,
      props.data.length
        ? props.data.map((item: any, index: number) =>
            React.createElement(
              React.Fragment,
              { key: props.keyExtractor(item) },
              props.renderItem({ item, index }),
            ),
          )
        : props.ListEmptyComponent,
      props.ListFooterComponent,
    );
  const modules = new Map<number, any>();
  const ports: Record<string, any> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      View: children,
      Text: () => null,
      ScrollView: children,
      FlatList: flatList,
      Pressable: Capture,
      ActivityIndicator: () => null,
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener: (_: string, callback: (value: string) => void) => {
          appListeners.add(callback);
          return { remove: () => appListeners.delete(callback) };
        },
      },
    },
    'expo-router': {
      useFocusEffect: focus.exports.useFocusEffect,
      Stack: { Screen: () => null },
      router: { push: (route: unknown) => routes.push(route) },
    },
    '@expo/ui': { BottomSheet: Capture, RNHostView: children },
  };
  const load = (index: number): any => {
    if (modules.has(index)) return modules.get(index);
    const module = { exports: {} };
    const resolve = (name: string): any => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name in ports) return ports[name];
      if (name.endsWith('/core/provider'))
        return {
          useClient: () => ({
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
            store,
          }),
        };
      if (name.endsWith('/account-scope')) return ownership;
      if (name === './session') return devices;
      if (name === './blocked-people') return blocked;
      if (name === './muted-chats') return muted;
      if (
        name.endsWith('/use-devices') ||
        name.endsWith('/use-blocked-people') ||
        name.endsWith('/use-muted-chats')
      )
        return load(1);
      if (
        name.endsWith('/device-list') ||
        name.endsWith('/blocked-people-view') ||
        name.endsWith('/muted-chats-screen')
      )
        return load(2);
      if (name.endsWith('/device-details') || name === './device-details') return load(3);
      if (name.endsWith('/components/dialogs'))
        return {
          useDialogs: () => ({
            confirm: (options: unknown) => {
              dialogs.push(options);
              return confirm(options);
            },
          }),
        };
      if (name.endsWith('/i18n')) return { t: (name: string) => name };
      if (name.endsWith('/theme'))
        return { useTheme: () => ({ presentation: telegramPresentation('day') }) };
      if (name.endsWith('/components/settings-rows')) return { SettingsRows: settingsRows };
      if (name.endsWith('/components/settings-screen')) return { SettingsScreen: children };
      if (name.endsWith('/profile-section')) return { ProfileSection: children };
      if (name.endsWith('/use-profile-photo')) return { useProfilePhoto: () => undefined };
      if (name.endsWith('/components/primitives'))
        return {
          EmptyState: () => null,
          Loading: () => null,
          Avatar: () => null,
          NativeButton: Capture,
          IconButton: Capture,
        };
      if (name.endsWith('/components/authorize-device')) return { AuthorizeDevice: () => null };
      if (name.endsWith('/components/history-transfer')) return { HistoryTransfer: () => null };
      throw Error(`Unexpected import ${name}`);
    };
    runInNewContext(compiled[kind][index]!, { module, exports: module.exports, require: resolve });
    modules.set(index, module.exports);
    return module.exports;
  };
  return {
    calls,
    routes,
    dialogs,
    errors: root.errors,
    other,
    person,
    render: () => root.render(React.createElement(load(0).default)),
    act: root.act,
    button: (key: string) => {
      const value = controls.get(key);
      assert.ok(value, key);
      return value;
    },
    has: (key: string) => controls.has(key),
    confirm: (fn: typeof confirm) => {
      confirm = fn;
    },
    foreground: (value: string) => {
      foreground = value;
      for (const callback of appListeners) callback(value);
    },
    focus: (value: boolean) => {
      focused = value;
      for (const callback of focusListeners[value ? 'focus' : 'blur']) callback();
    },
    reopen: async (change?: 'owner' | 'device' | 'relay') => {
      await store.close();
      if (change) {
        const account = snapshot.account!;
        if (change === 'owner') {
          const next = 'e'.repeat(64);
          snapshot.conversations = snapshot.conversations.map((chat) => ({
            ...chat,
            members: chat.members.map((member) =>
              member.id === account.identity_id ? { ...member, id: next } : member,
            ),
          }));
          snapshot.account = { ...account, identity_id: next };
        } else if (change === 'device') {
          const next = 'f'.repeat(64);
          directory = directory.map((row) => (row.this_device ? { ...row, device_id: next } : row));
          snapshot.account = { ...account, device_id: next };
        } else snapshot.account = { ...account, relay_url: 'https://other-relay.example' };
      }
      await store.open('account');
    },
    close: async () => {
      await root.close();
      await store.close();
    },
  };
}
