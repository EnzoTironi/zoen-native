import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import type { Dialogs } from '../components/dialog-types';

const path = fileURLToPath(new URL('../components/dialogs.ios.tsx', import.meta.url));
const code = (
  await build({
    stdin: {
      contents: readFileSync(path, 'utf8'),
      loader: 'tsx',
      resolveDir: fileURLToPath(new URL('../components/', import.meta.url)),
    },
    bundle: true,
    format: 'cjs',
    platform: 'node',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'native-dialog-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) =>
            path.startsWith('react') ||
            path.startsWith('@expo/') ||
            ['../theme', '../i18n', './native-dialogs'].includes(path)
              ? { path, external: true }
              : undefined,
          );
        },
      },
    ],
  })
).outputFiles[0]!.text;

test('the real iOS provider uses native destructive choices and retires exact presentations on abort and unmount', async () => {
  const root = createReactTestRoot();
  let api!: Dialogs;
  let host: any, dialog: any;
  const buttons = new Map<string, any>();
  const children = ({ children }: any) => React.createElement(React.Fragment, null, children);
  const confirmation = (props: any) => {
    dialog = props;
    return children(props);
  };
  const nativeConfirmation = Object.assign(confirmation, { Trigger: children, Actions: children });
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': jsxRuntime,
    '@expo/ui': {
      Host: (props: any) => {
        host = props;
        return children(props);
      },
    },
    '@expo/ui/swift-ui': {
      ConfirmationDialog: nativeConfirmation,
      VStack: () => null,
      Button: (props: any) => {
        React.useLayoutEffect(() => {
          buttons.set(props.label, props);
          return () => {
            buttons.delete(props.label);
          };
        }, [props]);
        return null;
      },
    },
    '@expo/ui/swift-ui/modifiers': { frame: (options: unknown) => options },
    '../theme': { useTheme: () => ({ scheme: 'dark' }) },
    '../i18n': { t: (key: string) => key },
    './native-dialogs': { nativeDialogs: { confirm: async () => false, inform: async () => {} } },
  };
  const module = {
    exports: {} as {
      DialogsProvider: React.ComponentType<{ children: React.ReactNode }>;
      useDialogs(): Dialogs;
    },
  };
  runInNewContext(code, {
    module,
    exports: module.exports,
    Promise,
    AbortController,
    require(name: string) {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  function Probe() {
    api = module.exports.useDialogs();
    return null;
  }
  let first!: Promise<string | null>, second!: Promise<string | null>;
  const options = {
    title: 'Delete?',
    choices: [
      { id: 'for_me', label: 'On this device', destructive: true },
      { id: 'everyone', label: 'For everyone', destructive: true },
    ],
  };
  try {
    await root.render(
      React.createElement(
        React.StrictMode,
        null,
        React.createElement(module.exports.DialogsProvider, null, React.createElement(Probe)),
      ),
    );
    await root.act(() => {
      first = api.choose(options);
      second = api.choose(options);
    });
    assert.equal(host.colorScheme, 'dark');
    assert.equal(dialog.titleVisibility, 'visible');
    assert.equal(dialog.isPresented, true);
    assert.deepEqual(
      Array.from(buttons.values(), (button) => button.role),
      ['destructive', 'destructive', 'cancel'],
    );
    const dismissOld = dialog.onIsPresentedChange;
    await root.act(() => buttons.get('On this device').onPress());
    assert.equal(await first, 'for_me');
    await root.act(() => dismissOld(false));
    assert.equal(dialog.title, 'Delete?');
    await root.act(() => buttons.get('For everyone').onPress());
    assert.equal(await second, 'everyone');
    const abort = new AbortController();
    let pending!: Promise<string | null>;
    await root.act(() => {
      pending = api.choose({ ...options, signal: abort.signal });
    });
    const lateChoice = buttons.get('For everyone').onPress;
    await root.act(() => abort.abort());
    assert.equal(await pending, null);
    await root.act(lateChoice);
    await root.act(() => {
      pending = api.choose(options);
    });
    await root.close();
    assert.equal(await pending, null);
    assert.equal(await api.choose(options), null);
    assert.deepEqual(root.errors, []);
  } catch (error) {
    await root.close();
    throw error;
  }
});
