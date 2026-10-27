import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import * as jsx from 'react/jsx-runtime';
import { createReactTestRoot } from '../core/react-test-root.fixtures.ts';
import * as formats from './binary-formats.ts';
import * as schema from './schema.ts';
import { installIntent } from './install-intent.ts';

const source = new URL('../app/miniapps/index.tsx', import.meta.url);
const compiled = await build({
  stdin: {
    contents: `${readFileSync(source, 'utf8')}\nexport { Install };`,
    sourcefile: fileURLToPath(source),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'native-install-ports',
      setup(b) {
        b.onResolve({ filter: /.*/ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});

/** Actual catalog component/React lifecycle; native controls are presentation ports. */
export async function binaryInstallFixture(editor: formats.BinaryEditorId) {
  const root = createReactTestRoot();
  const calls: Record<string, unknown>[] = [];
  const navigations: unknown[] = [];
  const texts: unknown[] = [];
  const buttons = new Map<string, { onPress: () => void }>();
  let release!: (value: string) => void;
  const held = new Promise<string>((resolve) => {
    release = resolve;
  });
  const store = {
    sessionEpoch: 1,
    async request(command: Record<string, unknown>) {
      calls.push(structuredClone(command));
      if (command.op === 'new_mutation_id') return held;
      assert.equal(command.op, 'install_app');
      return { id: 'synthetic-created' };
    },
  };
  const empty = () => null;
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': jsx,
    '@expo/ui': {
      Host: ({ children }: { children: React.ReactNode }) => children,
      Picker: Object.assign(empty, { Item: empty }),
    },
    'react-native': {
      ScrollView: empty,
      Text: ({ children }: { children: unknown }) => {
        texts.push(children);
        return null;
      },
    },
    'expo-router': {
      Stack: { Screen: empty },
      router: { replace: (route: unknown) => navigations.push(structuredClone(route)) },
    },
    '../../i18n/index.ts': { t: (key: string) => key },
    '../../core/provider': {
      useClient: () => ({
        store,
        snapshot: {
          conversations: [{ id: 'chat', title: 'Synthetic', members: [{ is_me: true }] }],
        },
      }),
    },
    '../../core/query': {},
    '../../components/primitives': {
      EmptyState: empty,
      Loading: empty,
      Surface: empty,
      NativeButton: (props: { label: string; onPress: () => void }) => {
        buttons.set(props.label, props);
        return null;
      },
    },
    '../../components/dialogs': { useDialogs: () => ({}) },
    '../../theme': { useTheme: () => ({ colors: {}, scheme: 'light' }) },
    '../../miniapps/schema-form': { SchemaForm: empty },
    '../../miniapps/schema': schema,
    '../../miniapps/install-intent': { installIntent },
    '../../miniapps/binary-formats': formats,
    '../../documents/use-dirty-editor': { useDirtyEditor: () => () => {} },
  };
  const module = {
    exports: {} as {
      Install: React.ComponentType<{ spec: unknown; initialSpace: string; onBack: () => void }>;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      assert.ok(Object.hasOwn(ports, name), name);
      return ports[name];
    },
  });
  await root.render(
    React.createElement(module.exports.Install, {
      spec: {
        id: editor,
        name: editor,
        description: '',
        tools: [{ visibility: ['model'], input_schema_json: '{"type":"object","properties":{}}' }],
      },
      initialSpace: 'chat',
      onBack() {},
    }),
  );
  return {
    root,
    calls,
    navigations,
    texts,
    store,
    async press() {
      const button = buttons.get('miniapps.addMiniapp');
      assert.ok(button, 'actual catalog install button must be rendered');
      await root.act(button.onPress);
    },
    async release() {
      await root.act(() => release('synthetic-mutation'));
    },
  };
}
