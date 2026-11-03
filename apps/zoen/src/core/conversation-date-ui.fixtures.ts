import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { deferred, draftSession } from './store.fixtures.ts';
import { conversationScope } from './conversation-scope.ts';
import { ConversationDateJump } from './conversation-date.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { common } from '../i18n/catalogs/common.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('../components/chat-date-navigation.ios.tsx', import.meta.url);
const compiled = transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.ok(compiled.includes('react/compiler-runtime'));

/** Actual Expo/React-compiled iOS sheet. Native controls are event ports, not pixels. */
export async function calendarUi(locale: 'en' | 'pt-BR' = 'en') {
  const root = createReactTestRoot();
  let snapshot = draftSession();
  let state: any = { phase: 'ready', mode: 'account', erasePending: false, snapshot };
  const listeners = new Set<() => void>(),
    cleanups = new Set<() => Promise<void>>();
  const controls = new Map<string, any>();
  const calls: any[] = [],
    revealed: string[] = [],
    selected: string[] = [],
    opened: number[] = [];
  const response = deferred<string | null>(),
    reveal = deferred<unknown>();
  const background = new Set<(state: string) => void>();
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    registerSessionCleanup: (cleanup: () => Promise<void>) => {
      cleanups.add(cleanup);
      return () => {
        cleanups.delete(cleanup);
      };
    },
    driver: {
      request: async (command: unknown) => {
        calls.push(structuredClone(command));
        return response.promise;
      },
    },
  };
  let client = { ...state, store };
  const id = snapshot.conversations[0]!.id;
  const t = createLocalization({ chat, common }, [locale]).t;
  const children = (props: any) => React.createElement(React.Fragment, null, props.children);
  const leaf = (name: string) => (props: any) => {
    controls.set(props.testID ?? props.label ?? name, props);
    return null;
  };
  const sheet = (props: any) => {
    controls.set('sheet', props);
    return children(props);
  };
  const app = {
    currentState: 'active',
    addEventListener: (_: string, listener: (state: string) => void) => {
      background.add(listener);
      return { remove: () => background.delete(listener) };
    },
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'expo-router': {
      useFocusEffect: (callback: () => () => void) => React.useEffect(callback, [callback]),
    },
    'react-native': { AppState: app, ScrollView: children, View: children, Text: leaf('text') },
    '@expo/ui': { Host: children, BottomSheet: sheet, RNHostView: children },
    '@expo/ui/community/datetime-picker': { __esModule: true, default: leaf('date') },
    '../core/conversation-scope': { conversationScope },
    '../core/conversation-date': { ConversationDateJump },
    '../core/provider': {
      useClient: () => {
        React.useSyncExternalStore(store.subscribe, () => client);
        return client;
      },
    },
    '../i18n': { t, formatLocale: locale },
    '../theme': { useTheme: () => ({ scheme: 'dark', presentation: { list: {} } }) },
    './primitives': { IconButton: leaf('icon'), NativeButton: leaf('button') },
  };
  const module = { exports: {} as { ChatDateNavigation: React.ComponentType<any> } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/')) return require(name);
      if (!(name in ports)) throw Error(name);
      return ports[name];
    },
  });
  await root.render(
    React.createElement(module.exports.ChatDateNavigation, {
      spaceId: id,
      onOpen: () => opened.push(1),
      reveal: async (target: string) => {
        revealed.push(target);
        return reveal.promise;
      },
      onSelect: (target: string) => selected.push(target),
    }),
  );
  return {
    root,
    calls,
    revealed,
    selected,
    opened,
    response,
    reveal,
    t,
    store,
    id,
    control: (id: string) => {
      const value = controls.get(id);
      assert.ok(value, `missing ${id}`);
      return value;
    },
    async background() {
      await root.act(() => {
        app.currentState = 'background';
        for (const listener of background) listener('background');
      });
    },
    async revoke() {
      await root.act(() => {
        snapshot = { ...snapshot, conversations: [] };
        state = { ...state, snapshot };
        client = { ...state, store };
        for (const listener of listeners) listener();
      });
    },
    close: () => root.close(),
  };
}
