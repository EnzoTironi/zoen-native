import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { EditorAgentSession } from '../miniapps/editor-agent-session.ts';
import * as port from '../miniapps/editor-agent-port.ts';
import * as formats from '../miniapps/binary-formats.ts';
import * as heads from '../miniapps/deck-version-selection.ts';
import { t } from '../i18n/index.ts';
import type { ItemDetail } from './types';

const require = createRequire(import.meta.url),
  { transformSync } = require('@babel/core');
type Control = Record<string, any>;
function compile(path: string) {
  const file = new URL(path, import.meta.url);
  const output = transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
  assert.match(output, /react\/compiler-runtime/);
  return output;
}
/** Real compiled screen/sheet + React/Store + real session/port; native controls
 * and Core are explicit ports, not a claim of iPhone presentation or AI execution. */
export async function editorAgentReactFixture() {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const space = snapshot.conversations[0]!.id,
    itemId = 'word-item';
  const calls: any[] = [],
    routes: any[] = [],
    controls = new Map<string, Control>();
  const item = {
    id: itemId,
    space_id: space,
    space_title: 'Conversation',
    title: 'My document',
    version: 1,
    app: { app_id: 'wordcraft' },
  } as ItemDetail;
  const spec = {
    id: 'wordcraft',
    has_view: true,
    tools: [],
    resource_uri: 'ui://zoen/wordcraft/index.html',
  };
  let flushes = 0,
    mounts = 0,
    unmounts = 0,
    ids = 0,
    presented = false;
  const document = {
    observedHead: async () => ({ itemId, version: 1, sha256: 'ab'.repeat(32) }),
    flush: async (options?: unknown) => {
      assert.equal(options, undefined);
      flushes++;
    },
  };
  driver.opening = async () => snapshot;
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    switch (command.op) {
      case 'snapshot':
        return snapshot;
      case 'requests':
        return [];
      case 'timeline':
        return { entries: [], before: null, has_more: false };
      case 'app_binary_read':
        return {
          item_id: itemId,
          kind: 'wordcraft',
          version: 1,
          current_version: 1,
          sha256: 'ab'.repeat(32),
          byte_length: 0,
          data_base64: '',
          engine_revision: formats.WORDCRAFT_ENGINE_REVISION,
        };
      case 'request_app_edit':
        return {
          entry: { id: 'canonical-request', kind: { Message: { text: command.text, card: null } } },
          conflict: null,
        };
      default:
        throw Error(`Unexpected operation ${command.op}`);
    }
  };
  const store = new ClientStore(driver);
  await store.open('account');
  calls.length = 0;
  const children = ({ children }: { children: React.ReactNode }) => children;
  function Capture(props: Control) {
    const key = props.testID;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (key && controls.get(key) === props) controls.delete(key);
      };
    }, [key, props]);
    return null;
  }
  function Sheet(props: Control) {
    React.useLayoutEffect(() => {
      presented = props.isPresented;
      controls.set('sheet', props);
    }, [props]);
    return props.isPresented ? props.children : null;
  }
  function Editor({ onDocumentController }: Control) {
    React.useEffect(() => {
      mounts++;
      onDocumentController(document);
      return () => {
        unmounts++;
        onDocumentController(null);
      };
    }, [onDocumentController]);
    return null;
  }
  const colors = {
    text: '#fff',
    secondary: '#888',
    danger: '#f00',
    background: '#111',
    surface: '#222',
    accent: '#0af',
  };
  const common: Record<string, any> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      View: children,
      Text: () => null,
      ScrollView: children,
      useWindowDimensions: () => ({ width: 402, height: 874 }),
      AppState: { addEventListener: () => ({ remove() {} }) },
    },
    '@expo/ui': { Host: children, RNHostView: children, BottomSheet: Sheet },
    'expo-crypto': { randomUUID: () => `request-${++ids}` },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ top: 40, bottom: 30, left: 0, right: 0 }),
    },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../theme': { useTheme: () => ({ colors, scheme: 'dark' }) },
    '../i18n': { t },
    '../i18n/index.ts': { t },
    '../components/primitives': {
      IconButton: Capture,
      NativeButton: Capture,
      Loading: () => null,
      Surface: children,
    },
    '../components/composer-text-field': { ComposerTextField: Capture },
    '../components/message-markdown': { MessageMarkdown: Capture },
    '../agents/request-card': { RequestCard: Capture },
    './app-sheet-context': {
      useAppSheetCloseButton: () => ({ top: 48, right: 12, width: 44, height: 44 }),
    },
    './document-controller': {},
    './editor-agent-port': port,
    './editor-agent-session': { EditorAgentSession },
    './binary-formats': formats,
    './deck-version-selection': heads,
  };
  function load(path: string, overrides: Record<string, any>) {
    const module = { exports: {} as any };
    runInNewContext(compile(path), {
      module,
      exports: module.exports,
      process: { env: { EXPO_OS: 'ios', NODE_ENV: 'production' } },
      require: (name: string) => {
        if (name.startsWith('@babel/runtime/')) return require(name);
        const value = { ...common, ...overrides }[name];
        if (!value) throw Error(`Unmocked ${name}`);
        return value;
      },
    });
    return module.exports;
  }
  const sheet = load('../miniapps/editor-agent-sheet.tsx', {});
  const screen = load('../miniapps/detail-screen.tsx', {
    'expo-router': {
      Stack: { Screen: () => null },
      router: {
        push: (...args: any[]) => routes.push(args),
        replace: (...args: any[]) => routes.push(args),
        back: () => routes.push('back'),
      },
      useLocalSearchParams: () => ({ id: itemId }),
      usePreventRemove: () => () => {},
    },
    '../core/query': {
      useCoreQuery: (command: Control) => ({
        data: command.op === 'item' ? item : [spec],
        retry() {},
        error: null,
      }),
    },
    '../components/dialogs': { useDialogs: () => ({ confirm: async () => true }) },
    '../home/provider': { useHome: () => ({}) },
    '../home/menus': { HomeCardMenu: () => null },
    '../native-surfaces/controls': { MiniappSurfaceControls: () => null },
    './conversation-provider': { useConversationCards: () => ({}) },
    './state-view': { MiniappState: () => null },
    './view-host': { MiniappView: Editor },
    './actions': { callMiniapp: async () => null },
    './editor-agent-sheet': sheet,
  });
  return {
    store,
    snapshot,
    calls,
    routes,
    errors: root.errors,
    controls,
    render: () => root.render(React.createElement(screen.MiniappScreen)),
    act: root.act,
    counters: () => ({ mounts, unmounts, flushes, presented }),
    close: async () => {
      await root.close();
      await store.close({ force: true });
    },
  };
}
