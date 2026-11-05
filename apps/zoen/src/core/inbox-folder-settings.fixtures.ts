import { build } from 'esbuild';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { ClientStore } from './store.ts';
import { Driver } from './store.fixtures.ts';
import { inboxFixture } from './inbox-actions.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { t } from '../i18n/index.ts';
const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [new URL('../inbox/folder-settings.tsx', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  jsx: 'automatic',
  plugins: [
    {
      name: 'native-folder-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (['react', 'react/jsx-runtime', 'react-native', '@expo/ui'].includes(path))
            return { path, external: true };
          for (const suffix of [
            '/core/provider',
            '/theme',
            '/components/settings-screen',
            '/components/settings-rows',
            '/components/primitives',
            '/i18n/index.ts',
          ]) {
            if (path.endsWith(suffix)) return { path: `fixture:${suffix}`, external: true };
          }
        });
      },
    },
  ],
});
export async function folderUi() {
  const root = createReactTestRoot(),
    { snapshot, chat } = inboxFixture(),
    driver = new Driver();
  snapshot.inbox_folders = {
    revision: '0',
    configuration: { folders: [], default_folder: 'all' },
    applied_revision: null,
  };
  const calls: any[] = [],
    controls = new Map<string, any>();
  let sequence = 0,
    loseAck = false,
    nextId: (() => Promise<string>) | null = null;
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command: any) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'new_mutation_id') return nextId ? nextId() : `intent-${++sequence}`;
    if (command.op === 'inbox_folders') {
      snapshot.inbox_folders = {
        configuration: structuredClone(command.input.configuration),
        revision: `revision-${sequence}`,
        applied_revision: `revision-${sequence}`,
      };
      if (loseAck) {
        loseAck = false;
        throw Error('Lost acknowledgement');
      }
      return structuredClone(snapshot.inbox_folders);
    }
    throw Error(`Unexpected command ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  function Capture(props: any) {
    const key = props.testID ?? props.label;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (controls.get(key) === props) controls.delete(key);
      };
    }, [key, props]);
    return React.createElement(React.Fragment, null, props.children);
  }
  const Picker = Object.assign(Capture, { Item: () => null });
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/jsx-runtime') return require(name);
      if (name === 'react-native') return { View: Capture, Text: () => null };
      if (name === '@expo/ui') return { Host: Capture, Picker, Switch: Capture };
      if (name === 'fixture:/core/provider')
        return {
          useClient: () => ({
            store,
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
          }),
        };
      if (name === 'fixture:/theme')
        return {
          useTheme: () => ({ scheme: 'dark', presentation: telegramPresentation('night') }),
        };
      if (name === 'fixture:/components/settings-screen') return { SettingsScreen: Capture };
      if (name === 'fixture:/components/settings-rows')
        return {
          SettingsRows: ({ rows }: any) =>
            React.createElement(
              React.Fragment,
              null,
              rows.map((row: any) =>
                React.createElement(Capture, { key: row.id, ...row, testID: row.id }),
              ),
            ),
        };
      if (name === 'fixture:/components/primitives')
        return { Field: Capture, NativeButton: Capture };
      if (name === 'fixture:/i18n/index.ts') return { t };
      throw Error(name);
    },
  });
  await root.render(React.createElement(module.exports.FolderSettings));
  return {
    root,
    store,
    calls,
    snapshot,
    chat,
    control(id: string) {
      const control = controls.get(id);
      if (!control) throw Error(`Missing ${id}; present: ${[...controls.keys()]}`);
      return control;
    },
    async press(id: string) {
      await root.act(() => controls.get(id)?.onPress());
    },
    async name(name: string) {
      await root.act(() => controls.get('folder-name').onChangeText(name));
    },
    loseAck() {
      loseAck = true;
    },
    nextId(fn: () => Promise<string>) {
      nextId = fn;
    },
    async close() {
      await root.close();
      await store.close({ force: true });
    },
  };
}
