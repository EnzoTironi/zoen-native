import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import * as account from './account-scope.ts';
import * as access from '../home/access.ts';
import * as workspace from '../documents/workspace-index.ts';
import * as resource from '../miniapps/canonical-resource.ts';
import * as packages from '../miniapps/package-intent.ts';
import * as formats from '../miniapps/binary-formats.ts';
import * as heads from '../miniapps/deck-version-selection.ts';
import * as appReview from '../miniapps/app-request-review.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { t } from '../i18n/index.ts';
import type { ItemDetail } from './types';

const require = createRequire(import.meta.url);
type Props = Record<string, any>;
const children = (p: Props) => p.children;

/** Actual Library row -> route -> management screen -> package sheet. Native
 * controls and Core reads are explicit ports; this is not a device/update proof. */
export async function workspaceManagementFixture(owned = true) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => snapshot;
  const store = new ClientStore(driver);
  await store.open('account');
  const item: ItemDetail = {
    id: 'drawing',
    space_id: snapshot.conversations[0]!.id,
    space_title: 'Conversation',
    kind_id: 'app',
    kind_label: 'App',
    title: 'Shared drawing',
    origin: '',
    created_by: snapshot.conversations[0]!.members[0]!,
    plan: null,
    text: null,
    version: 2,
    versions: [],
    linked_requests: [],
    path: '',
    file: null,
    app: {
      app_id: 'excalidraw',
      name: 'Excalidraw',
      resource_uri: owned
        ? `ui://zoen/package/file/1/${'ab'.repeat(32)}`
        : 'ui://zoen/excalidraw.html',
      view_json: '{}',
      headline: '',
      metrics: [],
      last_action: null,
      trust: 'Act',
      snapshot_json: '',
    },
  };
  const file: ItemDetail = {
    ...item,
    id: 'release',
    title: 'Updated drawing',
    kind_id: 'file',
    app: null,
    file: {
      name: 'Updated drawing',
      path: '',
      mime: 'text/html;profile=mcp-app',
      bytes: '1024',
      sha256: 'cd'.repeat(32),
      ready: true,
      chunks: 1,
      chunks_here: 1,
      has_thumbnail: false,
    },
  };
  const files = [
    file,
    { ...file, id: 'foreign', space_id: 'foreign-space' },
    { ...file, id: 'ordinary', file: { ...file.file!, mime: 'text/html' } },
  ];
  const controls = new Map<string, Props>(),
    routes: Props[] = [],
    reads: Props[] = [];
  const pressAncestors = React.createContext<string[]>([]);
  let navigate: (route: Props) => void = () => {},
    editors = 0;
  function Capture(p: Props) {
    const ancestors = React.useContext(pressAncestors);
    const key = p.testID ?? (p.label && `label:${p.label}`);
    React.useLayoutEffect(() => {
      if (key) controls.set(key, { ...p, ancestors });
      return () => {
        if (key) controls.delete(key);
      };
    });
    return null;
  }
  function Pressable(p: Props) {
    const ancestors = React.useContext(pressAncestors);
    return React.createElement(
      React.Fragment,
      null,
      React.createElement(Capture, p),
      React.createElement(
        pressAncestors.Provider,
        { value: [...ancestors, p.testID ?? 'pressable'] },
        p.children,
      ),
    );
  }
  const router = {
    push: (route: Props) => {
      routes.push(structuredClone(route));
      navigate(route);
    },
    replace: () => {
      throw Error('Unexpected replace');
    },
    back: () => {},
  };
  const common: Record<string, any> = {
    react: React,
    'react-native': {
      View: children,
      ScrollView: children,
      Pressable,
      Text: (p: Props) => React.createElement(Capture, p),
      useWindowDimensions: () => ({ fontScale: 1, width: 402, height: 874 }),
    },
    'expo-router': {
      router,
      Stack: { Screen: () => null },
      useLocalSearchParams: () => ({ id: item.id }),
      usePreventRemove: () => () => {},
      useFocusEffect: (effect: () => void | (() => void)) => React.useEffect(effect, [effect]),
    },
    '@expo/ui': {
      Host: children,
      RNHostView: children,
      BottomSheet: (p: Props) => (p.isPresented ? p.children : null),
    },
    '../components/primitives': {
      Icon: () => null,
      IconButton: Capture,
      NativeButton: Capture,
      Surface: children,
      Loading: () => null,
      EmptyState: Capture,
    },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../core/query': {
      useCoreQuery: (command: Props | null) => {
        if (command) reads.push(command);
        return {
          data: !command
            ? null
            : command.op === 'item'
              ? item
              : command.op === 'items'
                ? files
                : [{ id: 'excalidraw', has_view: true, tools: [] }],
          error: null,
          retry() {},
        };
      },
    },
    '../core/account-scope': account,
    '../home/access': access,
    '../theme': {
      useTheme: () => ({ colors: {}, scheme: 'dark', presentation: telegramPresentation('night') }),
    },
    '../i18n': { t },
    '../i18n/index.ts': { t },
    './workspace-index': workspace,
    '../miniapps/canonical-resource': resource,
    './canonical-resource': resource,
    './package-intent': packages,
    './package-action': {
      AppPackageAction: (p: Props) =>
        React.createElement(Capture, { testID: `package:${p.item.id}`, target: p.target }),
    },
    './binary-formats': formats,
    './deck-version-selection': heads,
    './app-request-review': appReview,
    '../components/dialogs': { useDialogs: () => ({ confirm: async () => true }) },
    '../home/provider': { useHome: () => ({}) },
    '../home/menus': { HomeCardMenu: () => null },
    '../native-surfaces/controls': { MiniappSurfaceControls: () => null },
    './conversation-provider': { useConversationCards: () => ({}) },
    './state-view': { MiniappState: () => null },
    './actions': { callMiniapp: () => {} },
    './editor-agent-sheet': { EditorAgentAccess: () => null },
    './view-host': {
      MiniappView: () => {
        editors++;
        return null;
      },
    },
  };
  function load(path: string) {
    const file = new URL(path, import.meta.url),
      module = { exports: {} as Props };
    const code = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
      filename: file.pathname,
      babelrc: false,
      configFile: false,
      presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
      plugins: [['babel-plugin-react-compiler', { target: '19' }]],
      caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
    }).code;
    if (path.endsWith('workspace-row.tsx') || path.endsWith('detail-screen.tsx'))
      assert.match(code, /react\/compiler-runtime/);
    runInNewContext(code, {
      module,
      exports: module.exports,
      process: { env: { EXPO_OS: 'ios' } },
      require(name: string) {
        if (
          name === 'react/compiler-runtime' ||
          name === 'react/jsx-runtime' ||
          name.startsWith('@babel/runtime/helpers/')
        )
          return require(name);
        if (name in common) return common[name];
        throw Error(`Unexpected management import: ${name}`);
      },
    });
    return module.exports;
  }
  const row = load('../documents/workspace-row.tsx');
  common['./package-release-sheet'] = load('../miniapps/package-release-sheet.tsx');
  common['../../../miniapps/detail-screen'] = load('../miniapps/detail-screen.tsx');
  const manage = load('../app/miniapp/[id]/manage.tsx');
  function App() {
    const [route, setRoute] = React.useState<Props | null>(null);
    React.useLayoutEffect(() => {
      navigate = setRoute;
      return () => {
        if (navigate === setRoute) navigate = () => {};
      };
    }, []);
    return route?.pathname === '/miniapp/[id]/manage'
      ? React.createElement(manage.default)
      : React.createElement(row.WorkspaceItemRow, {
          row: { type: 'item', key: 'item:drawing', item },
          openFolder() {},
        });
  }
  return {
    controls,
    routes,
    reads,
    item,
    errors: root.errors,
    render: () => root.render(React.createElement(App)),
    act: root.act,
    editors: () => editors,
    async reopen() {
      await store.close();
      await store.open('account');
    },
    async revoke() {
      snapshot.conversations = [];
      await store.refresh();
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
