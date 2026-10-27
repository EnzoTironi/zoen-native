import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession, deferred } from './store.fixtures.ts';
import { EphemeralConversation } from '../assistant/conversation.ts';
import * as scope from './conversation-scope.ts';
import type { AssistantOpen, AssistantPort } from '../assistant/types.ts';
export { deferred };
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const compile = (name: string) => {
  const file = new URL(name, import.meta.url);
  return transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
};
const compiled = {
  launch: compile('../assistant/launch.ts'),
  hook: compile('../assistant/use-conversation-assistant.ts'),
  route: compile('../app/ask/[space].tsx'),
};
const focusCode = readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8');
const loadedCode = readFileSync(
  require.resolve('expo-router/build/link/useLoadedNavigation.js'),
  'utf8',
);
type Composer = {
  value: string;
  sending: boolean;
  onChangeText(value: string): void;
  onSend(): void;
};
export async function assistantRouteFixture() {
  const root = createReactTestRoot();
  let snapshot = draftSession();
  const space = snapshot.conversations[0]!.id;
  const calls: unknown[] = [],
    opened: string[] = [],
    asked: string[] = [],
    closed: string[] = [];
  let openGate: Promise<AssistantOpen> | null = null,
    askGate: Promise<{ text: string }> | null = null,
    closeGate: Promise<void> | null = null;
  const assistant: AssistantPort = {
    async open(id) {
      opened.push(id);
      return (
        openGate ?? {
          available: true,
          id: `lease-${opened.length}`,
          contextMessages: 2,
          limited: true,
        }
      );
    },
    async ask(id, question) {
      asked.push(`${id}:${question}`);
      return askGate ?? { text: 'Local answer' };
    },
    async close(id) {
      closed.push(id);
      if (closeGate) await closeGate;
    },
  };
  const driver = Object.assign(new Driver(), { assistant });
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(command);
    if (command.op === 'snapshot') return structuredClone(snapshot);
    throw Error(`Unexpected mutation: ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open();
  let focused = true,
    foreground = 'active',
    ticket = '',
    uuid = 0;
  const appListeners = new Set<(value: string) => void>();
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', fn: () => void) {
      focusListeners[name].add(fn);
      return () => focusListeners[name].delete(fn);
    },
  };
  const navigationContext = React.createContext({});
  const loaded = { exports: {} };
  runInNewContext(loadedCode, {
    module: loaded,
    exports: loaded.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === '../react-navigation/native')
        return {
          useNavigation: () => navigation,
          NavigationContainerRefContext: navigationContext,
        };
      throw Error(name);
    },
  });
  const focus = { exports: {} };
  runInNewContext(focusCode, {
    module: focus,
    exports: focus.exports,
    process: { env: { NODE_ENV: 'production' } },
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
  });
  let composer: Composer | undefined;
  const text = new Map<string, unknown>(),
    buttons = new Map<string, { onPress(): void }>();
  const children = ({ children }: { children?: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children);
  const routes: { params: { ticket: string } }[] = [];
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'expo-crypto': { randomUUID: () => `ticket-${++uuid}` },
    'expo-router': {
      ...focus.exports,
      router: { push: (value: (typeof routes)[number]) => routes.push(value) },
      Stack: { Screen: () => null },
      useLocalSearchParams: () => ({ space, ticket }),
    },
    'expo-router/react-navigation': { useHeaderHeight: () => 64 },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ top: 20 }) },
    'react-native': {
      View: children,
      ScrollView: children,
      ActivityIndicator: () => null,
      Text: (props: { children?: unknown }) => {
        const id = React.useId();
        React.useLayoutEffect(() => {
          text.set(id, props.children);
          return () => {
            text.delete(id);
          };
        }, [id, props.children]);
        return null;
      },
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener: (_: string, fn: (value: string) => void) => {
          appListeners.add(fn);
          return { remove: () => appListeners.delete(fn) };
        },
      },
    },
    '../../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../core/conversation-scope': scope,
    './conversation': { EphemeralConversation },
    '../../assistant/conversation': { EphemeralConversation },
    '../../components/conversation-viewport': {
      ConversationContainer: children,
      ConversationViewport: (props: {
        composer: React.ReactNode;
        children: (spacer: null, scroll: null) => React.ReactNode;
      }) => React.createElement(React.Fragment, null, props.children(null, null), props.composer),
    },
    '../../components/message-composer': {
      MessageComposer: (props: Composer) => {
        React.useLayoutEffect(() => {
          composer = props;
          return () => {
            if (composer === props) composer = undefined;
          };
        }, [props]);
        return null;
      },
    },
    '../../components/primitives': {
      Loading: () => null,
      EmptyState: children,
      NativeButton: (props: { label: string; onPress(): void }) => {
        React.useLayoutEffect(() => {
          buttons.set(props.label, props);
          return () => {
            buttons.delete(props.label);
          };
        }, [props]);
        return null;
      },
    },
    '../../i18n/index.ts': { locale: 'en', t: (key: string) => key },
    '../../theme': { useTheme: () => ({ colors: {} }) },
  };
  ports['../core/provider'] = ports['../../core/provider'];
  function load(code: string) {
    const module = { exports: {} as Record<string, any> };
    runInNewContext(code, {
      module,
      exports: module.exports,
      TextEncoder,
      requestAnimationFrame: () => 1,
      cancelAnimationFrame: () => {},
      process: { env: { EXPO_OS: 'ios' } },
      require(name: string) {
        if (name in ports) return ports[name];
        if (name.startsWith('@babel/runtime/helpers/')) return require(name);
        throw Error(`Unexpected import ${name}`);
      },
    });
    return module.exports;
  }
  const launch = load(compiled.launch);
  ports['../../assistant/launch'] = launch;
  ports['../../assistant/use-conversation-assistant'] = load(compiled.hook);
  const Route = load(compiled.route).default;
  return {
    root,
    store,
    space,
    calls,
    opened,
    asked,
    closed,
    routes,
    buttons,
    text: () => [...text.values()],
    composer: () => composer,
    launch: (query = 'Missing search question') => {
      launch.launchConversationAssistant(space, query, store);
      ticket = routes.at(-1)!.params.ticket;
    },
    mount: () => root.render(React.createElement(Route)),
    focus: (next: boolean) =>
      root.act(() => {
        focused = next;
        for (const fn of focusListeners[next ? 'focus' : 'blur']) fn();
      }),
    foreground: (next: string) =>
      root.act(() => {
        foreground = next;
        for (const fn of appListeners) fn(next);
      }),
    changeOwner: () =>
      root.act(async () => {
        snapshot = { ...snapshot, account: { ...snapshot.account!, device_id: 'another-device' } };
        await store.refresh();
      }),
    removeMembership: () =>
      root.act(async () => {
        snapshot = { ...snapshot, conversations: [] };
        await store.refresh();
      }),
    reopen: () => root.act(() => store.open()),
    restoreOriginal: () =>
      root.act(async () => {
        snapshot = draftSession();
        await store.refresh();
      }),
    release: () => launch.releaseAssistantLaunch(ticket),
    holdOpen: (gate: Promise<AssistantOpen> | null) => {
      openGate = gate;
    },
    holdAsk: (gate: Promise<{ text: string }>) => {
      askGate = gate;
    },
    holdClose: (gate: Promise<void> | null) => {
      closeGate = gate;
    },
    cleanup: async () => {
      await root.close();
      await store.close();
      assert.deepEqual(root.errors, []);
    },
  };
}
