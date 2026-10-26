import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { appRequestReviewState } from './app-request-review-state.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import * as account from './account-scope.ts';
import * as decision from './tool-request-decision.ts';
import * as renewal from './tool-review-renewal.ts';
import * as appReview from '../miniapps/app-request-review.ts';
import * as port from '../miniapps/editor-agent-port.ts';
import { EditorAgentSession } from '../miniapps/editor-agent-session.ts';
import * as formats from '../miniapps/binary-formats.ts';
import * as heads from '../miniapps/deck-version-selection.ts';
import * as admission from '../miniapps/editor-agent-admission.ts';
import * as memberFlow from '../communities/member-actions.ts';
import * as addition from '../communities/member-addition.ts';
import * as memberPort from '../communities/member-context-port.ts';
import * as source from '../sites/source.ts';
import * as enrollment from '../agent-enrollment/session.ts';
import * as enrollmentPort from '../agent-enrollment/core-port.ts';
import * as trust from '../agents/trust-confirmation.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { t, formatLocale } from '../i18n/index.ts';

const require = createRequire(import.meta.url);
type Props = Record<string, any>;
type Route = { pathname: string; params: Props };
const children = ({ children }: Props) => children;

/** The actual standalone route, App route, screen, sheet, sessions and drawing
 * review port run in React. Core/native controls/frame rendering are explicit
 * boundaries; no iPhone pixels, inference or signed crypto authority is claimed. */
export async function appRequestReviewReactFixture(options: { canonicalFlush?: boolean } = {}) {
  const model = await appRequestReviewState(options),
    root = createReactTestRoot();
  const controls = new Map<string, Props>(),
    routes: Route[] = [],
    params: Props[] = [];
  const decisionErrors: string[] = [];
  class ObservedEditorAgentSession extends EditorAgentSession {
    constructor(...args: ConstructorParameters<typeof EditorAgentSession>) {
      super(...args);
      const decide = this.decide;
      this.decide = async (...input: Parameters<EditorAgentSession['decide']>) => {
        try {
          return await decide(...input);
        } catch (reason) {
          decisionErrors.push(reason instanceof Error ? reason.message : String(reason));
          throw reason;
        }
      };
    }
  }
  const RouteContext = React.createContext<Route>({
    pathname: '/request/[id]',
    params: { id: model.request.id },
  });
  const visibility = React.createContext(true);
  let changeRoute: (value: Route | ((old: Route) => Route)) => void = () => {};
  let routeNow: Route = { pathname: '/request/[id]', params: { id: model.request.id } };
  let focused = true,
    foreground = 'active',
    editorMounts = 0,
    editorUnmounts = 0;
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const appListeners = new Set<(value: string) => void>();
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', callback: () => void) {
      focusListeners[name].add(callback);
      return () => focusListeners[name].delete(callback);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as Props };
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
        throw Error(`Unexpected loaded-navigation import ${name}`);
      },
    },
  );
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    process: { env: { NODE_ENV: 'production' } },
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(`Unexpected focus import ${name}`);
    },
  });
  function Capture(props: Props) {
    const visible = React.useContext(visibility);
    const key = visible
      ? (props.testID ?? (props.label ? `label:${props.label}` : undefined))
      : null;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (key && controls.get(key) === props) controls.delete(key);
      };
    }, [key, props]);
    return null;
  }
  function VisibleView(props: Props) {
    const shown = React.useContext(visibility);
    const hidden = [props.style].flat().some((style) => style?.display === 'none');
    return React.createElement(visibility.Provider, { value: shown && !hidden }, props.children);
  }
  function Sheet(props: Props) {
    React.useLayoutEffect(() => {
      controls.set('sheet', props);
    }, [props]);
    return props.isPresented ? props.children : null;
  }
  function Editor({ onDocumentController }: Props) {
    React.useEffect(() => {
      editorMounts++;
      onDocumentController(model.document);
      return () => {
        editorUnmounts++;
        onDocumentController(null);
      };
    }, [onDocumentController]);
    return null;
  }
  const router = {
    push(route: Route) {
      routes.push(structuredClone(route));
      changeRoute(route);
    },
    replace(route: Route) {
      routes.push(structuredClone(route));
      changeRoute(route);
    },
    back() {
      changeRoute({ pathname: '/request/[id]', params: { id: model.request.id } });
    },
    setParams(patch: Props) {
      params.push(structuredClone(patch));
      changeRoute((old) => {
        const next = { ...old.params, ...patch };
        for (const key in next) if (next[key] === undefined) delete next[key];
        return { ...old, params: next };
      });
    },
  };
  const colors = {
    text: '#fff',
    secondary: '#888',
    danger: '#f00',
    background: '#111',
    surface: '#222',
    accent: '#0af',
  };
  const common: Props = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      View: VisibleView,
      ScrollView: children,
      Pressable: (p: Props) =>
        React.createElement(React.Fragment, null, React.createElement(Capture, p), p.children),
      Text: (p: Props) =>
        React.createElement(Capture, {
          ...p,
          testID: typeof p.children === 'string' ? `text:${p.children}` : undefined,
        }),
      FlatList: (p: Props) =>
        React.createElement(
          React.Fragment,
          null,
          p.data.map((item: unknown, index: number) =>
            React.createElement(React.Fragment, { key: index }, p.renderItem({ item })),
          ),
        ),
      Keyboard: { dismiss() {} },
      Platform: { OS: 'ios' },
      useWindowDimensions: () => ({ width: 402, height: 874, fontScale: 1 }),
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener(_: string, callback: (value: string) => void) {
          appListeners.add(callback);
          return { remove: () => appListeners.delete(callback) };
        },
      },
    },
    'expo-router': {
      router,
      Stack: { Screen: () => null },
      useLocalSearchParams: () => React.useContext(RouteContext).params,
      usePreventRemove: () => () => {},
      useFocusEffect: focus.exports.useFocusEffect,
    },
    '@expo/ui': {
      Host: children,
      RNHostView: children,
      BottomSheet: Sheet,
      Picker: Object.assign(
        (p: Props) =>
          React.createElement(React.Fragment, null, React.createElement(Capture, p), p.children),
        { Item: Capture },
      ),
    },
    'expo-image': { Image: Capture },
    'expo-crypto': { randomUUID: () => 'generated-id' },
    'expo-camera': {
      CameraView: {
        isModernBarcodeScannerAvailable: true,
        onModernBarcodeScanned: () => ({ remove() {} }),
        dismissScanner: async () => {},
        launchScanner: async () => {},
      },
      useCameraPermissions: () => [{ granted: true }, async () => ({ granted: true })],
    },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ top: 40, bottom: 30, left: 0, right: 0 }),
    },
    '../core/provider': {
      useClient: () => ({
        store: model.store,
        ...React.useSyncExternalStore(
          model.store.subscribe,
          model.store.getSnapshot,
          model.store.getSnapshot,
        ),
      }),
    },
    '../core/account-scope': account,
    '../core/account-scope.ts': account,
    '../core/tool-request-decision': decision,
    '../core/tool-review-renewal.ts': renewal,
    '../core/presentation': { money: () => '' },
    '../core/image-tool-review': { imageToolReviewValid: () => false },
    '../sites/source': source,
    '../theme': {
      useTheme: () => ({ colors, scheme: 'dark', presentation: telegramPresentation('night') }),
    },
    '../i18n': { t },
    '../i18n/index.ts': { t, formatLocale },
    '../components/primitives': {
      NativeButton: Capture,
      IconButton: Capture,
      Icon: () => null,
      Avatar: () => null,
      Field: Capture,
      Surface: children,
      Loading: () => null,
      EmptyState: Capture,
    },
    '../components/composer-text-field': { ComposerTextField: Capture },
    '../components/message-markdown': { MessageMarkdown: Capture },
    '../components/dialogs': {
      useDialogs: () => ({ confirm: async () => assert.fail('No stacked confirmation') }),
    },
    '../components/settings-rows': {
      SettingsRows: (p: Props) =>
        React.createElement(
          React.Fragment,
          null,
          p.rows.map((row: Props) =>
            React.createElement(Capture, { ...row, key: row.id, testID: row.id }),
          ),
        ),
    },
    '../components/settings-screen': { SettingsScreen: children },
    '../profile/profile-avatar': { ProfileAvatar: () => null },
    '../profile/profile-header': { ProfileHeader: () => null },
    '../profile/profile-section': { ProfileSection: children },
    './member-actions': memberFlow,
    './member-addition': addition,
    './member-context-port': memberPort,
    './session': enrollment,
    './core-port': enrollmentPort,
    '../agents/trust-confirmation': trust,
    './editor-agent-admission': admission,
    './editor-agent-port': port,
    './editor-agent-session': { EditorAgentSession: ObservedEditorAgentSession },
    './app-request-review': appReview,
    '../miniapps/app-request-review': appReview,
    './binary-formats': formats,
    './deck-version-selection': heads,
    './app-sheet-context': {
      useAppSheetCloseButton: () => ({ top: 48, right: 12, width: 44, height: 44 }),
    },
    './package-release-sheet': { AppPackageReleaseSheet: () => null },
    './site-tool-review': { SiteToolReview: () => assert.fail('Scene cannot render Site review') },
  };
  common['./provider'] = common['../core/provider'];
  const load = (path: string, overrides: Props = {}) =>
    loadEditorAgent(path, { ...common, ...overrides });
  const query = load('../core/query.ts');
  common['../core/query'] = common['../../core/query'] = query;
  common['../../core/provider'] = common['../core/provider'];
  common['../../components/primitives'] = common['../components/primitives'];
  common['../../i18n/index.ts'] = common['../i18n/index.ts'];
  common['../communities/use-member-actions'] = load('../communities/use-member-actions.ts');
  common['../communities/member-addition-sheet'] = load('../communities/member-addition-sheet.tsx');
  common['../agent-enrollment/connect-agent-view'] = load(
    '../agent-enrollment/connect-agent-view.tsx',
  );
  common['../agent-enrollment/use-agent-enrollment'] = load(
    '../agent-enrollment/use-agent-enrollment.ts',
  );
  common['./use-tool-review'] = load('../agents/use-tool-review.ts');
  common['./tool-review-recovery'] = load('../agents/tool-review-recovery.tsx');
  common['./drawing-tool-review'] = load('../agents/drawing-tool-review.tsx');
  common['./app-request-review-action'] = load('../agents/app-request-review-action.tsx');
  common['../agents/request-card'] = common['../../agents/request-card'] = load(
    '../agents/request-card.tsx',
  );
  common['./editor-agent-choice'] = load('../miniapps/editor-agent-choice.tsx');
  common['./editor-agent-permissions'] = load('../miniapps/editor-agent-permissions.tsx', {
    '../components/dialogs': { useDialogs: () => ({ confirm: model.targets.confirm }) },
  });
  common['./editor-agent-content'] = load('../miniapps/editor-agent-content.tsx');
  common['./editor-agent-enrollment'] = load('../miniapps/editor-agent-enrollment.tsx');
  common['./use-app-request-review-handoff'] = load(
    '../miniapps/use-app-request-review-handoff.ts',
  );
  common['./editor-agent-sheet'] = load('../miniapps/editor-agent-sheet.tsx');
  common['../home/provider'] = { useHome: () => ({}) };
  common['../home/menus'] = { HomeCardMenu: () => null };
  common['../native-surfaces/controls'] = { MiniappSurfaceControls: () => null };
  common['./conversation-provider'] = { useConversationCards: () => ({}) };
  common['./state-view'] = { MiniappState: () => null };
  common['./actions'] = {
    callMiniapp: () => assert.fail('Review navigation cannot call an App mutation'),
  };
  common['./view-host'] = { MiniappView: Editor };
  common['../../miniapps/detail-screen'] = load('../miniapps/detail-screen.tsx');
  const appRoute = load('../app/miniapp/[id].tsx'),
    requestRoute = load('../app/request/[id].tsx');
  function App() {
    const [route, setRoute] = React.useState(routeNow);
    React.useLayoutEffect(() => {
      changeRoute = setRoute;
      routeNow = route;
    }, [route]);
    return React.createElement(
      RouteContext.Provider,
      { value: route },
      React.createElement(
        route.pathname === '/miniapp/[id]' ? appRoute.default : requestRoute.default,
      ),
    );
  }
  return {
    ...model,
    controls,
    routes,
    params,
    decisionErrors,
    errors: root.errors,
    render: () => root.render(React.createElement(App)),
    act: root.act,
    route: () => routeNow,
    editorMounts: () => editorMounts,
    editorUnmounts: () => editorUnmounts,
    navigate(route: Route) {
      changeRoute(route);
    },
    focus(value: boolean) {
      if (focused === value) return;
      focused = value;
      for (const f of focusListeners[value ? 'focus' : 'blur']) f();
    },
    foreground(value: string) {
      foreground = value;
      for (const f of appListeners) f(value);
    },
    async close() {
      await root.close();
      await model.close();
    },
  };
}
