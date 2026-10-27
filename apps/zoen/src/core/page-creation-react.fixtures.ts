import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession, deferred } from './store.fixtures.ts';
import * as ownership from './account-scope.ts';
import * as home from '../home/access.ts';
import * as authority from '../documents/document-authority.ts';
import * as activity from '../documents/export-activity.ts';
import type { SpaceMemberContext } from './generated/SpaceMemberContext';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const compile = (path: string) => {
  const file = new URL(path, import.meta.url);
  return transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
    caller: {
      name: 'metro',
      platform: 'ios',
      engine: 'hermes',
      isDev: false,
      supportsReactCompiler: true,
    },
  }).code as string;
};
const pageCode = compile('../app/new-page.tsx');
assert.match(pageCode, /react\/compiler-runtime/);
const markdownCode = (
  await build({
    entryPoints: [new URL('../documents/markdown-files.ts', import.meta.url).pathname],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'native-file-ports',
        setup(builder) {
          builder.onResolve({ filter: /^(expo-|\.\.\/i18n\/)/ }, ({ path }) => ({
            path,
            external: true,
          }));
        },
      },
    ],
  })
).outputFiles[0]!.text;

/** Real React/Expo focus, ClientStore and compiled NewPage/query/Markdown port.
 * Native hosts, picker/files and Core boundary are synthetic, never device proof. */
export async function pageCreationFixture(
  role = 'member',
  kind: 'Group' | 'Community' | 'Direct' = 'Group',
  patch: Partial<SpaceMemberContext> = {},
) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const room = snapshot.conversations[0]!;
  room.kind = kind;
  const second = {
    ...structuredClone(room),
    id: 'second-space',
    title: 'Second chat',
    kind: 'Group' as const,
  };
  snapshot.conversations.push(second);
  const roles = new Map([
    [room.id, role],
    [second.id, 'member'],
  ]);
  const states = new Map<string, SpaceMemberContext['state']>();
  const calls: any[] = [],
    created: any[] = [],
    routes: any[] = [];
  let readGate: Promise<SpaceMemberContext> | null = null;
  let pickerGate: Promise<any> | null = null;
  let fileGate: Promise<string> | null = null;
  const text = '# Exact 👩🏽‍💻\r\n\r\n**rich** [link](https://example.test)\r\n';
  let deleted = 0;
  const context = (space = room.id): SpaceMemberContext => ({
    identity_id: snapshot.account!.identity_id,
    device_id: snapshot.account!.device_id,
    space_id: space,
    space_title: space === room.id ? room.title : second.title,
    member_id: snapshot.account!.identity_id,
    member: room.members.find((m) => m.id === snapshot.account!.identity_id) ?? null,
    role: roles.get(space) ?? null,
    state: states.get(space) ?? 'Active',
    can_remove: false,
    can_manage_members: false,
    can_add: false,
    owned_agent: false,
    delivery_ready: true,
    candidate: null,
    addition_failed: false,
    removal_failed: false,
    role_options: [],
    pending_role: null,
    role_change_failed: false,
    revision: 'a'.repeat(64),
    ...patch,
  });
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'space_member_context') {
      if (
        command.expected_identity_id !== snapshot.account!.identity_id ||
        command.expected_device_id !== snapshot.account!.device_id ||
        command.identity_id !== snapshot.account!.identity_id
      )
        throw Error('Member context owner changed');
      const gate = readGate;
      readGate = null;
      return gate ?? context(command.space_id);
    }
    if (command.op === 'page_create' || command.op === 'page_import_markdown') {
      if (roles.get(command.space_id) === 'reader') throw Error('only members can create pages');
      const item = { id: `page-${created.length + 1}`, space_id: command.space_id };
      created.push({ ...structuredClone(command), item });
      return item;
    }
    throw Error(`Unexpected Page operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  let foreground = 'active',
    focused = true;
  const lifecycle = new Set<(state: string) => void>();
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', fn: () => void) {
      focusListeners[name].add(fn);
      return () => focusListeners[name].delete(fn);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as { useFocusEffect: unknown } };
  const load = (code: string, ports: Record<string, unknown>) => {
    const module = { exports: {} as any };
    runInNewContext(code, {
      module,
      exports: module.exports,
      require(name: string) {
        if (name.startsWith('@babel/runtime/helpers/')) return require(name);
        assert.ok(name in ports, name);
        return ports[name];
      },
      process: { env: { EXPO_OS: 'ios', NODE_ENV: 'production' } },
      console,
      TextEncoder,
      TextDecoder,
    });
    return module.exports;
  };
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
  const buttons = new Map<string, any>(),
    radios = new Map<string, any>(),
    fields = new Map<string, any>();
  const messages = new Map<string, unknown>();
  function useRecord(map: Map<string, any>, key: string, props: any) {
    React.useLayoutEffect(() => {
      map.set(key, props);
      return () => {
        if (map.get(key) === props) map.delete(key);
      };
    }, [map, key, props]);
  }
  function NativeButton(props: any) {
    useRecord(buttons, props.label, props);
    return null;
  }
  function Field(props: any) {
    useRecord(fields, props.label, props);
    return null;
  }
  function Text(props: any) {
    useRecord(messages, React.useId(), props.children);
    return null;
  }
  function Pressable(props: any) {
    const child = React.Children.toArray(props.children)[0] as React.ReactElement<{
      children: string;
    }>;
    useRecord(radios, child.props.children, props);
    return React.createElement(React.Fragment, null, props.children);
  }
  const passthrough = (props: any) => React.createElement(React.Fragment, null, props.children);
  const File = class {
    exists = true;
    size = new TextEncoder().encode(text).length;
    async text() {
      return fileGate ?? text;
    }
    delete() {
      this.exists = false;
      deleted++;
    }
  };
  const markdown = load(markdownCode, {
    'expo-document-picker': {
      getDocumentAsync: () =>
        pickerGate ??
        Promise.resolve({
          canceled: false,
          assets: [{ uri: 'file:///fixture/selected.md', name: 'selected.md' }],
        }),
    },
    'expo-file-system': { File },
    'expo-sharing': {},
    '../i18n/index.ts': { t: (key: string) => key },
    '../i18n/money.ts': { formatMoneyInput: () => '' },
  });
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      Pressable,
      ScrollView: passthrough,
      Text,
      View: passthrough,
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener(_: string, fn: (s: string) => void) {
          lifecycle.add(fn);
          return { remove: () => lifecycle.delete(fn) };
        },
      },
    },
    'expo-router': {
      Stack: { Screen: () => null },
      router: { replace: (route: unknown) => routes.push(route) },
      useLocalSearchParams: () => ({ space: room.id }),
      useFocusEffect: focus.exports.useFocusEffect,
    },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    './provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../core/account-scope': ownership,
    '../home/access': home,
    './document-authority': authority,
    './export-activity': activity,
    '../i18n/index.ts': { t: (key: string) => key },
    '../theme': { useTheme: () => ({ colors: {} }) },
    '../components/primitives': { Field, NativeButton, Loading: () => null },
    '../documents/markdown-files': markdown,
    '../documents/use-dirty-editor': { useDirtyEditor: () => () => {} },
  };
  const query = load(compile('./query.ts'), ports);
  ports['../core/query'] = query;
  Object.defineProperty(ports, '../documents/use-page-creation-destination', {
    get: () => load(compile('../documents/use-page-creation-destination.ts'), ports),
  });
  const page = load(pageCode, ports);
  await root.render(React.createElement(page.default));
  const get = (map: Map<string, any>, label: string) => {
    const result = map.get(label);
    assert.ok(result, label);
    return result;
  };
  return {
    root,
    store,
    snapshot,
    room,
    second,
    calls,
    created,
    routes,
    text,
    context,
    roles,
    states,
    get deleted() {
      return deleted;
    },
    button: (label: string) => get(buttons, label),
    field: (label: string) => get(fields, label),
    messages: () => [...messages.values()],
    async title(value = 'A Page') {
      await root.act(() => get(fields, 'documents.title').onChangeText(value));
    },
    async pick() {
      await root.act(() => get(buttons, 'documents.importMarkdownFile').onPress());
    },
    async select(secondSpace = true) {
      await root.act(() => get(radios, secondSpace ? second.title : room.title).onPress());
    },
    holdRead() {
      const gate = deferred<SpaceMemberContext>();
      readGate = gate.promise;
      return gate;
    },
    holdPicker() {
      const gate = deferred<any>();
      pickerGate = gate.promise;
      return {
        resolve: () =>
          gate.resolve({
            canceled: false,
            assets: [{ uri: 'file:///fixture/selected.md', name: 'selected.md' }],
          }),
      };
    },
    holdFileRead() {
      const gate = deferred<string>();
      fileGate = gate.promise;
      return { resolve: () => gate.resolve(text) };
    },
    async refresh() {
      await root.act(() => store.refresh([room.id]));
    },
    async focus(next: boolean) {
      await root.act(() => {
        focused = next;
        for (const fn of focusListeners[next ? 'focus' : 'blur']) fn();
      });
    },
    async background(next: string) {
      await root.act(() => {
        foreground = next;
        for (const fn of lifecycle) fn(next);
      });
    },
    async reopen() {
      await root.act(() => store.open('account'));
    },
    async replaceOwner(kind: 'identity' | 'device' | 'epoch') {
      if (kind === 'identity') {
        const before = snapshot.account!.identity_id;
        snapshot.account = { ...snapshot.account!, identity_id: 'c'.repeat(64) };
        for (const chat of snapshot.conversations)
          chat.members = chat.members.map((m) =>
            m.id === before ? { ...m, id: snapshot.account!.identity_id } : m,
          );
      }
      if (kind === 'device') snapshot.account = { ...snapshot.account!, device_id: 'e'.repeat(64) };
      await root.act(() => store.open('account'));
    },
    async close() {
      await root.close();
      await store.close();
      assert.deepEqual(root.errors, []);
    },
  };
}
