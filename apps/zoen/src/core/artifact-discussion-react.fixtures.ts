import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import * as discussion from '../documents/discussion.ts';
import * as access from '../home/access.ts';
import { t } from '../i18n/index.ts';
import type { ArtifactDiscussion } from '../documents/artifact-discussion.tsx';
import type { ItemDiscussionDto } from './generated/ItemDiscussionDto';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
function compile(path: string) {
  const file = new URL(path, import.meta.url);
  const source = transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
  assert.match(source, /react\/compiler-runtime/);
  return source;
}
const component = compile('../documents/artifact-discussion.tsx'),
  query = compile('./query.ts');
type Control = { label: string; disabled?: boolean; onPress(): void };
/** Real compiled component, Core query, React/Expo focus and Store. Native
 * controls are captured ports, not evidence of native rendering or touch delivery. */
export async function discussionReactFixture(open = true) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const spaceId = snapshot.conversations[0]!.id,
    itemId = 'page';
  const routes: unknown[] = [],
    calls: string[] = [];
  let replyCount = 2,
    pending: Promise<ItemDiscussionDto> | null = null;
  const value = (): ItemDiscussionDto => ({
    space_id: spaceId,
    item_id: itemId,
    root: {
      id: 'original-root',
      in_thread: null,
      thread_replies: replyCount,
    } as ItemDiscussionDto['root'],
  });
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(command.op);
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'item_discussion') {
      const gate = pending;
      pending = null;
      return gate ?? value();
    }
    throw Error(`Unexpected discussion operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  if (open) await store.open('account');
  let foreground = 'active',
    focused = true;
  const appListeners = new Set<(next: string) => void>();
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
      require(name: string) {
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
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  let button: Control | null = null;
  function NativeButton(props: Control) {
    React.useLayoutEffect(() => {
      button = props;
      return () => {
        if (button === props) button = null;
      };
    }, [props]);
    return null;
  }
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      View: ({ children }: { children: React.ReactNode }) => children,
      Text: () => null,
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener(_: string, callback: (next: string) => void) {
          appListeners.add(callback);
          return { remove: () => appListeners.delete(callback) };
        },
      },
    },
    'expo-router': {
      useFocusEffect: focus.exports.useFocusEffect,
      router: { push: (route: unknown) => routes.push(structuredClone(route)) },
    },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../components/primitives': { NativeButton },
    '../home/access': access,
    './discussion': discussion,
    '../i18n/index': { t },
    '../theme': { useTheme: () => ({ colors: {} }) },
  };
  ports['./provider'] = ports['../core/provider'];
  function load<T>(source: string): T {
    const module = { exports: {} };
    runInNewContext(source, {
      module,
      exports: module.exports,
      require(name: string) {
        if (name.startsWith('@babel/runtime/helpers/')) return require(name);
        assert.ok(name in ports, name);
        return ports[name];
      },
    });
    return module.exports as T;
  }
  ports['../core/query'] = load(query);
  const loadedComponent = load<{ ArtifactDiscussion: typeof ArtifactDiscussion }>(component);
  return {
    store,
    snapshot,
    routes,
    calls,
    errors: root.errors,
    act: root.act,
    value,
    button: () => button,
    render: () =>
      root.render(React.createElement(loadedComponent.ArtifactDiscussion, { spaceId, itemId })),
    open: () => root.act(() => store.open('account')),
    read(gate: Promise<ItemDiscussionDto>) {
      pending = gate;
    },
    async reopen(change?: 'identity' | 'device' | 'relay') {
      await root.act(async () => {
        await store.close();
        if (change === 'identity') {
          snapshot.account = { ...snapshot.account!, identity_id: 'e'.repeat(64) };
          snapshot.conversations[0]!.members[0]!.id = snapshot.account.identity_id;
        }
        if (change === 'device')
          snapshot.account = { ...snapshot.account!, device_id: 'f'.repeat(64) };
        if (change === 'relay')
          snapshot.account = { ...snapshot.account!, relay_url: 'https://other-relay.invalid/' };
        replyCount = 5;
        await store.open('account');
      });
    },
    foreground(next: string) {
      foreground = next;
      for (const fn of appListeners) fn(next);
    },
    focus(next: boolean) {
      focused = next;
      for (const fn of focusListeners[next ? 'focus' : 'blur']) fn();
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
