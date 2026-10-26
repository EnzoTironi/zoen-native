import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { deferred } from './store.fixtures.ts';

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
  '../core/query',
  '../core/provider',
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

async function fixture(query = '') {
  const root = createReactTestRoot();
  const state: any = {
    phase: 'ready',
    mode: 'demo',
    erasePending: false,
    snapshot: { me: { id: 'me' }, conversations: [{ id: 'chat', members: [{ id: 'me' }] }] },
  };
  const listeners = new Set<() => void>();
  const pending = deferred<any>();
  let list: any,
    input: any,
    avoiding: any,
    retries = 0,
    dismissals = 0;
  const routes: any[] = [],
    reads: any[] = [];
  const store = {
    sessionEpoch: 0,
    isSessionRetiring: false,
    getSnapshot: () => state,
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    driver: {
      request: (command: any) => {
        reads.push(command);
        return pending.promise;
      },
    },
  };
  let data: any = {
      hits: [
        { kind: 'message', space_id: 'chat', ref_id: 'message', title: 'Match', snippet: 'text' },
      ],
    },
    error: string | null = null;
  const queryListeners = new Set<() => void>();
  let projection: { data: unknown; error: string | null } = { data, error };
  const module = { exports: {} as any };
  const Container = ({ children }: any) => React.createElement(React.Fragment, null, children);
  const EmptyState = () => null,
    Loading = () => null;
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    setTimeout,
    clearTimeout,
    process: { env: { EXPO_OS: 'ios' } },
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
          useFocusEffect: (effect: () => any) => React.useEffect(effect, [effect]),
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
      if (name === '../core/provider') return { useClient: () => ({ store }) };
      if (name === '../core/query')
        return {
          useCoreQuery: (command: any) => {
            const result = React.useSyncExternalStore(
              (listener) => {
                queryListeners.add(listener);
                return () => {
                  queryListeners.delete(listener);
                };
              },
              () => projection,
            );
            return {
              data: command ? result.data : null,
              error: command ? result.error : null,
              loading: false,
              retry: () => {
                retries++;
              },
            };
          },
        };
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
  return {
    root,
    render,
    store,
    state,
    pending,
    routes,
    reads,
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
      return retries;
    },
    async result(next: any, failure: string | null = null) {
      await root.act(() => {
        projection = { data: next, error: failure };
        queryListeners.forEach((listener) => listener());
      });
    },
    revoke() {
      state.snapshot.conversations = [];
      listeners.forEach((fn) => fn());
    },
  };
}

test('compiled search keeps empty/loading/error states inside its keyboard-safe list', async () => {
  const f = await fixture();
  try {
    assert.equal(f.avoiding.behavior, 'padding');
    assert.equal(f.avoiding.automaticOffset, true);
    assert.equal(f.list.keyboardShouldPersistTaps, 'handled');
    assert.equal(f.list.keyboardDismissMode, 'interactive');
    assert.equal(f.list.contentContainerStyle.flexGrow, 1);
    assert.equal(f.list.contentContainerStyle.paddingBottom, 50);
    assert.equal(f.list.ListEmptyComponent.type, f.EmptyState);
    await f.root.act(() => f.input.onChangeText('missing'));
    assert.equal(f.list.ListEmptyComponent.type, f.Loading);
    await f.result(null, 'offline');
    await f.root.act(() => new Promise((resolve) => setTimeout(resolve, 210)));
    assert.ok(f.list.ListHeaderComponent);
    assert.equal(f.list.ListEmptyComponent, null);
    await f.root.act(() => f.list.ListHeaderComponent.props.children[1].props.onPress());
    assert.equal(f.retries, 1);
    await f.result({ hits: [] });
    await f.render();
    assert.equal(f.list.ListEmptyComponent.props.title, 'chat.noResults');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});

test('compiled result opening preserves query and resolves the canonical thread before navigation', async () => {
  const f = await fixture('old query');
  try {
    await f.root.act(() => f.list.renderItem({ item: f.list.data[0] }).props.onPress());
    assert.equal(f.reads.length, 1);
    await f.root.act(async () => {
      f.pending.resolve({ id: 'message', in_thread: 'root', deleted: false });
      await f.pending.promise;
    });
    assert.equal(f.input.value, 'old query');
    assert.deepEqual(JSON.parse(JSON.stringify(f.routes)), [
      { pathname: '/thread/[space]/[root]', params: { space: 'chat', root: 'root' } },
    ]);
    assert.equal(f.dismissals, 1);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.root.close();
  }
});

for (const retirement of ['query', 'unmount', 'account', 'access-ABA'] as const)
  test(`compiled result cannot navigate or dismiss keyboard after ${retirement}`, async () => {
    const f = await fixture('old query');
    try {
      await f.root.act(() => f.list.renderItem({ item: f.list.data[0] }).props.onPress());
      if (retirement === 'query') await f.root.act(() => f.input.onChangeText('new query'));
      else if (retirement === 'unmount') await f.root.render(null);
      else if (retirement === 'account') {
        f.store.sessionEpoch++;
        await f.render();
      } else {
        await f.root.act(() => f.revoke());
        f.state.snapshot.conversations = [{ id: 'chat', members: [{ id: 'me' }] }];
      }
      await f.root.act(async () => {
        f.pending.resolve({ id: 'message', in_thread: null });
        await f.pending.promise;
      });
      assert.equal(f.routes.length, 0);
      assert.equal(f.dismissals, 0);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.root.close();
    }
  });
