import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import { createReactTestRoot } from '../core/react-test-root.fixtures.ts';
import { deferred } from '../core/store.fixtures.ts';
import type { Command, ItemDetail } from '../core/types.ts';
import type { AppPackageAction } from './package-action';

const require = createRequire(import.meta.url);
const portNames = new Set([
  'react',
  'react/jsx-runtime',
  'expo-router',
  'expo-crypto',
  'react-native',
  '../core/provider',
  '../components/primitives',
  '../theme',
  '../i18n/index',
  '../i18n/index.ts',
]);
const compiled = await build({
  entryPoints: [new URL('./package-action.tsx', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'native-core-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          portNames.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

/** Real React component/lifecycle; Core acceptance and native controls remain explicit ports. */
export function packageActionFixture(mime = 'text/html;profile=mcp-app', update = false) {
  const root = createReactTestRoot();
  const install = deferred<void>();
  let accepted: ItemDetail | null = null,
    effects = 0,
    loseAck = false,
    held = false;
  let updated = 0;
  let epoch = 1,
    focused = true;
  const owner = { identity_id: 'owner', device_id: 'device', unlocked: true, setup_pending: false };
  const members = [{ id: 'owner' }];
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: { account: owner, me: { id: 'owner' }, conversations: [{ id: 'space', members }] },
  };
  const changed = new Set<() => void>(),
    appChanged = new Set<(state: string) => void>();
  const buttons = new Map<string, { onPress(): void; disabled?: boolean }>();
  const commands: Command[] = [],
    routes: unknown[] = [];
  const file = {
    id: 'file',
    kind_id: 'file',
    space_id: 'space',
    version: 1,
    title: 'Shared App',
    file: { mime, sha256: 'a'.repeat(64), ready: true },
  } as ItemDetail;
  const app = {
    id: 'canonical-app',
    space_id: 'space',
    app: { app_id: 'excalidraw', resource_uri: `ui://zoen/package/file/1/${'a'.repeat(64)}` },
  } as ItemDetail;
  const target = update
    ? {
        ...app,
        title: 'Existing document',
        app: { ...app.app!, resource_uri: `ui://zoen/package/prior/1/${'d'.repeat(64)}` },
      }
    : undefined;
  const document = {
    item_id: 'canonical-app',
    version: 3,
    current_version: 3,
    sha256: 'b'.repeat(64),
    scene_json: '{"elements":[]}',
  };
  const store = {
    get sessionEpoch() {
      return epoch;
    },
    isSessionRetiring: false,
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      changed.add(listener);
      return () => changed.delete(listener);
    },
    registerSessionCleanup: () => () => {},
    request: async (command: Command) => {
      commands.push(command);
      if (command.op === 'new_mutation_id') return 'install-intent';
      if (command.op === 'app_resource_find') return accepted;
      if (command.op === 'app_document_read_pinned') return document;
      if (command.op === 'app_resource_update') {
        effects++;
        if (held) await install.promise;
        return { ...document, version: 4, current_version: 4 };
      }
      if (command.op === 'app_resource_install') {
        effects++;
        accepted = app;
        if (held) await install.promise;
        if (loseAck) {
          loseAck = false;
          throw Error('lost ACK');
        }
        return accepted;
      }
      throw Error('Unexpected command');
    },
  };
  const appState = {
    currentState: 'active',
    addEventListener(_name: string, listener: (state: string) => void) {
      appChanged.add(listener);
      return { remove: () => appChanged.delete(listener) };
    },
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'expo-router': {
      router: { push: (route: unknown) => routes.push(route) },
      useFocusEffect(callback: () => void | (() => void)) {
        const isFocused = focused;
        React.useEffect(() => (isFocused ? callback() : undefined), [callback, isFocused]);
      },
    },
    'react-native': {
      AppState: appState,
      View: ({ children }: { children: React.ReactNode }) =>
        React.createElement(React.Fragment, null, children),
      Text: () => null,
    },
    '../core/provider': { useClient: () => ({ store }) },
    '../components/primitives': {
      NativeButton: (props: { testID: string; onPress(): void }) => {
        buttons.set(props.testID, props);
        return null;
      },
    },
    '../theme': { useTheme: () => ({ colors: { danger: '#f00' } }) },
    '../i18n/index': { t: (key: string) => key },
    '../i18n/index.ts': { t: (key: string) => key },
  };
  const module = { exports: {} as { AppPackageAction: typeof AppPackageAction } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => (Object.hasOwn(ports, name) ? ports[name] : require(name)),
    console,
  });
  const render = () =>
    root.render(
      React.createElement(module.exports.AppPackageAction, {
        item: file,
        target,
        onUpdated: () => {
          updated++;
        },
      }),
    );
  const actionId = update ? 'app-package-update' : 'app-package-open';
  return {
    commands,
    routes,
    errors: root.errors,
    render,
    effects: () => effects,
    updated: () => updated,
    hold: () => {
      held = true;
    },
    loseAck: () => {
      loseAck = true;
    },
    release: () => install.resolve(),
    button: () => buttons.get(actionId),
    press: (times = 1) =>
      root.act(() => {
        const button = buttons.get(actionId);
        assert.ok(button);
        for (let i = 0; i < times; i++) button.onPress();
      }),
    settle: () => root.act(() => Promise.resolve()),
    async background() {
      await root.act(() => {
        appState.currentState = 'background';
        appChanged.forEach((f) => f('background'));
      });
    },
    async foreground() {
      await root.act(() => {
        appState.currentState = 'active';
        appChanged.forEach((f) => f('active'));
      });
    },
    async revokeRestore() {
      await root.act(() => {
        members.splice(0);
        changed.forEach((f) => f());
        members.push({ id: 'owner' });
        changed.forEach((f) => f());
      });
    },
    async changeOwner() {
      await root.act(() => {
        epoch++;
        owner.identity_id = 'new-owner';
        changed.forEach((f) => f());
      });
    },
    async blur() {
      focused = false;
      await render();
    },
    async focus() {
      focused = true;
      await render();
    },
    close: () => root.close(),
  };
}
