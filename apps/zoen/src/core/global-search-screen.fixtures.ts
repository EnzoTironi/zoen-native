import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { deferred, Driver, draftSession } from './store.fixtures.ts';
import { ClientStore } from './store.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const ports = new Set([
  'react',
  'react/jsx-runtime',
  'react/compiler-runtime',
  'expo-router',
  'expo-router/react-navigation',
  'react-native',
  'react-native-keyboard-controller',
  'react-native-safe-area-context',
  '@shopify/flash-list',
  '../i18n/index.ts',
  '../profile/profile-avatar',
  '../core/provider',
  './provider',
  '../theme',
  '../components/primitives',
]);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../app/search.tsx', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'compiled-search',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
        builder.onLoad({ filter: /search\.tsx$/ }, ({ path }) => ({
          contents: transformSync(readFileSync(path, 'utf8'), {
            filename: path,
            babelrc: false,
            configFile: false,
            presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
            plugins: [['babel-plugin-react-compiler', { target: '19' }]],
            caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
          }).code,
          loader: 'js',
        }));
      },
    },
  ],
});

export async function fixture(
  query = '',
  mode: 'demo' | 'account' = 'demo',
  initialResults?: any,
  platform = 'ios',
  canonicalStore = false,
) {
  const root = createReactTestRoot();
  const state: any = {
    phase: 'ready',
    mode,
    erasePending: false,
    snapshot: {
      account:
        mode === 'account'
          ? { identity_id: 'me', device_id: 'device', unlocked: true, setup_pending: false }
          : null,
      me: { id: 'me' },
      conversations: [{ id: 'chat', members: [{ id: 'me' }] }],
    },
  };
  const listeners = new Set<() => void>();
  const pending = deferred<any>();
  let list: any,
    input: any,
    avoiding: any,
    dismissals = 0;
  const routes: any[] = [],
    reads: any[] = [],
    queries: any[] = [];
  let searchRead: ((command: any) => Promise<any>) | null = null;
  let store: any = {
    sessionEpoch: 0,
    isSessionRetiring: false,
    getSnapshot: () => state,
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    subscribeCanonicalChanges: () => () => {},
    driver: {
      request: (command: any) => {
        if (['universal_search', 'universal_search_page', 'search_messages'].includes(command.op)) {
          queries.push(command);
          if (searchRead) return searchRead(command);
          return error
            ? Promise.reject(new Error(error))
            : Promise.resolve({ hits: data?.hits ?? [], next_cursor: data?.next_cursor ?? null });
        }
        reads.push(command);
        return pending.promise;
      },
    },
  };
  let data: any = initialResults ?? {
      hits: [
        { kind: 'message', space_id: 'chat', ref_id: 'message', title: 'Match', snippet: 'text' },
      ],
    },
    error: string | null = null;
  let focused = true;
  const focusListeners = new Set<(value: boolean) => void>();
  function useFocusEffect(effect: () => void | (() => void)) {
    React.useEffect(() => {
      let cleanup = focused ? effect() : undefined;
      const changed = (value: boolean) => {
        cleanup?.();
        cleanup = value ? effect() : undefined;
      };
      focusListeners.add(changed);
      return () => {
        cleanup?.();
        focusListeners.delete(changed);
      };
    }, [effect]);
  }
  let canonicalSnapshot: any;
  if (canonicalStore) {
    const base = draftSession();
    canonicalSnapshot = {
      ...base,
      account: { ...base.account, ...state.snapshot.account, registered: true },
      me: { ...base.me, id: 'me' },
      conversations: [
        {
          ...base.conversations[0],
          id: 'chat',
          members: [{ ...base.conversations[0]!.members[0], id: 'me' }],
        },
      ],
    };
    const driver = new Driver(),
      request = store.driver.request;
    driver.opening = async () => structuredClone(canonicalSnapshot);
    driver.operation = async (command) =>
      command.op === 'snapshot' ? structuredClone(canonicalSnapshot) : request(command);
    store = new ClientStore(driver);
    await store.open('account');
  }
  const module = { exports: {} as any };
  const Container = ({ children }: any) => React.createElement(React.Fragment, null, children);
  const EmptyState = () => null,
    Loading = () => null;
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    setTimeout,
    clearTimeout,
    process: { env: { EXPO_OS: platform } },
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
          Stack: { Screen: () => null },
          router: { push: (route: any) => routes.push(route) },
          useLocalSearchParams: () => ({ query }),
          useFocusEffect,
        };
      if (name === 'expo-router/react-navigation') return { useHeaderHeight: () => 88 };
      if (name === 'react-native')
        return {
          View: Container,
          Text: () => null,
          Pressable: () => null,
          TextInput: (props: any) => {
            input = props;
            return null;
          },
        };
      if (name === 'react-native-keyboard-controller')
        return {
          KeyboardAvoidingView: (props: any) => {
            avoiding = props;
            return React.createElement(Container, props);
          },
          KeyboardController: {
            dismiss: async () => {
              dismissals++;
            },
          },
        };
      if (name === 'react-native-safe-area-context')
        return { useSafeAreaInsets: () => ({ bottom: 34 }) };
      if (name === '@shopify/flash-list')
        return {
          FlashList: (props: any) => {
            list = props;
            return null;
          },
        };
      if (name === '../core/provider' || name === './provider')
        return { useClient: () => ({ store, ...(canonicalStore ? store.getSnapshot() : state) }) };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === '../profile/profile-avatar') return { ProfileAvatar: () => null };
      if (name === '../components/primitives')
        return { EmptyState, Loading, NativeButton: () => null };
      throw Error(name);
    },
  });
  const render = () => root.render(React.createElement(module.exports.default));
  await render();
  if (query) await root.act(() => new Promise((resolve) => setTimeout(resolve, 220)));
  return {
    root,
    render,
    store,
    state,
    pending,
    routes,
    reads,
    queries,
    canonicalSnapshot,
    searchRead: (fn: typeof searchRead) => {
      searchRead = fn;
    },
    focus: (value: boolean) => {
      focused = value;
      focusListeners.forEach((fn) => fn(value));
    },
    EmptyState,
    Loading,
    get list() {
      return list;
    },
    get input() {
      return input;
    },
    get avoiding() {
      return avoiding;
    },
    get dismissals() {
      return dismissals;
    },
    get retries() {
      return Math.max(0, queries.length - 1);
    },
    async result(next: any, failure: string | null = null) {
      data = next;
      error = failure;
    },
    revoke() {
      state.snapshot.conversations = [];
      listeners.forEach((fn) => fn());
    },
  };
}
