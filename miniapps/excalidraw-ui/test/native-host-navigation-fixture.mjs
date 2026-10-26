import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const fixtureSource = `
import React, { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import AppsLayout from 'production:app-layout';
import { MiniappScreen } from 'production:detail-screen';
import { useNavigationTreeReducer } from 'expo-source:global-state/useNavigationTreeReducer';
import { useNavigationTreeReportEvents } from 'expo-source:global-state/useNavigationTreeReportEvents';
import { RemovalPreventionProvider, PreventRemovalProvider, useRoutesWithRemovalPrevented } from 'expo-source:global-state/removalPrevention';
import { NavigationContext } from 'expo-source:react-navigation/core/NavigationContext';
import { NavigationRouteContext } from 'expo-source:react-navigation/core/NavigationProvider';
import { StackRouter } from 'expo-source:react-navigation/routers/StackRouter';
import { createImperativeRouter, router } from 'expo-source:global-state/router';
import { useBackButton } from 'expo-source:fork/useBackButton.native';
import { qa } from 'fixture:state';

const browser = { apply() {} };
function stack(key, names, routes) {
  return { key, type: 'stack', stale: false, routeKeySeq: 10, routeNames: names, routes, index: routes.length - 1 };
}
function register(state, registry) {
  const router = StackRouter({});
  registry.set(state.key, {
    reduce: (current, action) => router.getStateForAction(current, action, { routeNames: current.routeNames, routeGetIdList: {} }),
    getStateForRouteFocus: router.getStateForRouteFocus,
    shouldActionChangeFocus: router.shouldActionChangeFocus,
  });
  for (const route of state.routes) if (route.state) register(route.state, registry);
}
function NavigationHarness() {
  const [, render] = useState(0);
  qa.rerender = () => render(value => value + 1);
  const prevented = useRoutesWithRemovalPrevented();
  const tree = useNavigationTreeReducer({ initialState: qa.initial, registry: qa.registry, routesWithRemovalPrevented: prevented, linking: { getPathFromState: () => '/fixture' } });
  useNavigationTreeReportEvents(tree.report, tree.consumeReportEvents, browser);
  qa.tree = tree;
  qa.prevented = prevented;
  qa.navigation.getState = () => tree.state;
  qa.navigation.canGoBack = () => tree.state.routes.length > 1;
  qa.navigation.dispatch = (action) => { qa.dispatched.push(action); tree.handleAction(action); };
  // Native focus/ref is a port; the actual Expo listener, reducer, and screen guard run below.
  const rootRef = useRef(null);
  rootRef.current = {
    canGoBack: () => tree.state.routes.length > 1,
    dispatchSync: (action) => tree.handleAction(action),
  };
  qa.rootRef = rootRef;
  useBackButton(rootRef);
  Object.assign(router, createImperativeRouter((intent) => tree.processIntent(intent), () => {}));
  const visible = tree.state.routes[tree.state.index].name;
  if (visible !== 'miniapp') return <div data-qa-destination={visible}>{visible}</div>;
  qa.route = <PreventRemovalProvider routeKey="drawing" emitRemovalEvent={(_key, type, action) => qa.navigation.emit(type, { data: { action } })}><MiniappScreen /></PreventRemovalProvider>;
  return <NavigationContext.Provider value={qa.navigation}>
    <NavigationRouteContext.Provider value={qa.routeIdentity}>
      <PreventRemovalProvider routeKey="app-modal"><AppsLayout /></PreventRemovalProvider>
    </NavigationRouteContext.Provider>
  </NavigationContext.Provider>;
}

export function mount({ controller, management = false, origin = 'home', loading = false }) {
  const events = new Map();
  const backHandlers = new Set();
  Object.assign(qa, {
    controller, loading, backHandlers, nativeFallbacks: 0, disposedDocuments: 0,
    dispatched: [], focused: true, path: management ? '/miniapp/synthetic-drawing/manage' : '/miniapp/synthetic-drawing', routeIdentity: {key:'app-modal',name:'miniapp'},
    navigation: {
      getId() {return '';},
      isFocused() {return qa.focused;},
      addListener(type, callback) { const listeners = events.get(type) ?? new Set(); events.set(type, listeners); listeners.add(callback); return () => listeners.delete(callback); },
      emit(type, event) { for (const callback of [...(events.get(type) ?? [])]) callback(event); },
    },
  });
  const appRoutes = [{ key: 'drawing', name: '[id]' }];
  if (management) appRoutes.push({ key: 'management', name: '[id]/manage' });
  const rootRoutes = [{ key: 'home', name: 'home' }];
  if (origin === 'chat') rootRoutes.push({ key: 'chat', name: 'chat' });
  rootRoutes.push({ key: 'app-modal', name: 'miniapp', state: stack('apps-stack', ['[id]', '[id]/manage'], appRoutes) });
  qa.initial = stack('root-stack', ['home', 'chat', 'miniapp'], rootRoutes);
  qa.registry = new Map(); register(qa.initial, qa.registry);
  const node = document.createElement('div'); document.body.append(node);
  const root = createRoot(node); root.render(<RemovalPreventionProvider><NavigationHarness /></RemovalPreventionProvider>);
  return {
    qa, node,
    back() { for (const callback of [...backHandlers].reverse()) if (callback()) return true; qa.nativeFallbacks++; return false; },
    focus(value) {qa.focused=value;qa.navigation.emit(value?'focus':'blur',{});},
    path(value) {qa.path=value;qa.rerender();},
    unmount() { root.unmount(); node.remove(); },
  };
}
`;
const repo = process.env.ZOEN_BACK_REPO ?? fileURLToPath(new URL("../../../", import.meta.url));
const app = join(repo, "apps/zoen");
const appRequire = createRequire(join(app, "package.json"));
const miniRequire = createRequire(join(repo, "miniapps/excalidraw-ui/package.json"));
const { build } = miniRequire("esbuild");
const { JSDOM } = miniRequire("jsdom");
const React = appRequire("react");
const expo = join(dirname(appRequire.resolve("expo-router/package.json")), "build");
const detail = join(app, "src/miniapps/detail-screen.tsx");
const appRequestReview = join(app, "src/miniapps/app-request-review.ts");
const layout = join(app, "src/app/miniapp/_layout.tsx");
const chrome = join(app, "src/miniapps/app-sheet-chrome.tsx");

const stateSource = "export const qa = {};";
const bridgeSource = `
export { router } from 'expo-source:global-state/router';
export { usePreventRemove } from 'expo-source:react-navigation/core/usePreventRemove';
export { useFocusEffect } from 'expo-source:useFocusEffect';
export { useNavigation } from 'expo-source:useNavigation';
import React from 'react';import {qa}from'fixture:state';
export function Stack({children}){return React.createElement(React.Fragment,null,children,qa.route)};Stack.Screen=()=>null;
export function useLocalSearchParams(){ return {id:'synthetic-drawing'}; }
export function usePathname(){return qa.path;}
`;
const nativeSource = `
import React from 'react'; import {qa} from 'fixture:state';
export const Platform={OS:'android'};
export const BackHandler={addEventListener(type,callback){if(type!=='hardwareBackPress')throw Error(type);qa.backHandlers.add(callback);return{remove(){qa.backHandlers.delete(callback)}}}};
export const StyleSheet={absoluteFill:{position:'absolute',top:0,right:0,bottom:0,left:0}};
export function View({children,...props}){return React.createElement('div',{'data-qa-native-view':props.testID},children)};
export function Text({children}){return React.createElement('span',null,children)};
export const ScrollView=View;
`;
const viewSource = `
import React,{useEffect}from'react';import{qa}from'fixture:state';import{useSafeAreaInsets}from'react-native-safe-area-context';import{useAppSheetCloseButton}from'production:sheet-context';
export function MiniappView({onDocumentController}){qa.editorInsets=useSafeAreaInsets();qa.editorChrome=useAppSheetCloseButton();useEffect(()=>{onDocumentController?.(qa.controller);return()=>{qa.disposedDocuments++;onDocumentController?.(null)}},[onDocumentController]);return React.createElement('div',{'data-qa-editor':'ready'});}
`;
const portsSource = `
import React from'react';import{qa}from'fixture:state';
export const t=(key)=>key;
export function useClient(){return{store:{}}};
export function useCoreQuery(input){return{data:input.op==='item'?(qa.loading?undefined:{id:'synthetic-drawing',title:'Synthetic drawing',space_id:'synthetic-chat',app:{app_id:'excalidraw'}}):[{id:'excalidraw',has_view:true,tools:[]}],retry(){}}};
export function useTheme(){return{colors:{danger:'#b00',secondary:'#888'}}};
export function useHome(){return{items:[],isPinned(){return false}}};
export function useDialogs(){return{}};
export function useConversationCards(){return{items:[],isPinned(){return false}}};
export const MiniappState=()=>null,MiniappSurfaceControls=()=>null,HomeCardMenu=()=>null,EditorAgentAccess=()=>null;
export const AppPackageReleaseSheet=()=>null;
export const callMiniapp=()=>{throw Error('Unexpected app tool')};
export const EmptyState=({title,children})=>React.createElement('div',null,title,children);
export const Loading=()=>React.createElement('div',{'data-qa-loading':'true'});
export const NativeButton=({label,onPress})=>React.createElement('button',{onClick:onPress},label);
export const Surface=({children})=>React.createElement('div',null,children);
export const SkyScene=({children})=>React.createElement(React.Fragment,null,children);
`;

async function fixture(platform = "android", options = {}) {
  const dom = new JSDOM("<!doctype html><body></body>", { url: "http://fixture.invalid" });
  const prior = Object.fromEntries(
    ["window", "document", "navigator", "IS_REACT_ACT_ENVIRONMENT"].map((name) => [
      name,
      Object.getOwnPropertyDescriptor(globalThis, name),
    ]),
  );
  for (const [key, value] of Object.entries({
    window: dom.window,
    document: dom.window.document,
    navigator: dom.window.navigator,
    IS_REACT_ACT_ENVIRONMENT: true,
  }))
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  const bundled = await build({
    stdin: { contents: fixtureSource, resolveDir: repo, loader: "jsx" },
    bundle: true,
    platform: "node",
    format: "cjs",
    write: false,
    jsx: "automatic",
    packages: "external",
    loader: { ".png": "dataurl" },
    define: { "process.env.EXPO_OS": JSON.stringify(platform) },
    plugins: [
      {
        name: "native-boundaries-only",
        setup(builder) {
          builder.onResolve({ filter: /^react(?:\/.*)?$|^react-dom(?:\/.*)?$/ }, ({ path }) => ({
            path,
            external: true,
          }));
          builder.onResolve({ filter: /^expo-source:/ }, ({ path }) => ({
            path: join(
              expo,
              path
                .slice(12)
                .replace(
                  platform === "web" ? /useBackButton\.native$/ : /__NONE__/,
                  "useBackButton",
                ) + ".js",
            ),
          }));
          builder.onResolve({ filter: /^production:/ }, ({ path }) => ({
            path:
              path === "production:app-layout"
                ? layout
                : path === "production:sheet-context"
                  ? join(app, "src/miniapps/app-sheet-context.ts")
                  : detail,
          }));
          builder.onResolve({ filter: /^fixture:/ }, ({ path }) => ({
            path,
            namespace: "fixture",
          }));
          builder.onLoad({ filter: /.*/, namespace: "fixture" }, ({ path }) => ({
            contents:
              path === "fixture:state"
                ? stateSource
                : path === "fixture:bridge"
                  ? bridgeSource
                  : path === "fixture:native"
                    ? nativeSource.replace("OS:'android'", `OS:${JSON.stringify(platform)}`)
                    : path === "fixture:view"
                      ? viewSource
                      : path === "fixture:gestures"
                        ? gesturesSource
                        : portsSource,
            loader: "jsx",
          }));
          builder.onResolve({ filter: /^react-native-safe-area-context$/ }, () => ({
            path: "fixture:safe-area",
            namespace: "safe-area",
          }));
          builder.onLoad({ filter: /.*/, namespace: "safe-area" }, () => ({
            contents:
              "import React from'react';export const SafeAreaInsetsContext=React.createContext({top:24,right:0,bottom:16,left:0});export const useSafeAreaInsets=()=>React.useContext(SafeAreaInsetsContext);export function SafeAreaProvider({children,...props}){if(Object.keys(props).length)throw Error('App provider must measure its own frame');return React.createElement('div',{'data-qa-local-safe-provider':'true'},children)};",
          }));
          builder.onResolve({ filter: /.*/ }, ({ path, importer }) =>
            importer === layout && path.startsWith(".")
              ? path.endsWith("/app-sheet-chrome")
                ? { path: chrome }
                : { path: "fixture:ports", namespace: "fixture" }
              : importer === chrome && (path === "../i18n/index.ts" || path === "../theme")
                ? { path: "fixture:ports", namespace: "fixture" }
                : undefined,
          );
          builder.onResolve({ filter: /^\.\.\/react-navigation\/native$/ }, ({ importer }) =>
            importer === join(expo, "fork/useBackButton.native.js")
              ? { path: "fixture:common-actions", namespace: "common-actions" }
              : undefined,
          );
          builder.onLoad({ filter: /.*/, namespace: "common-actions" }, () => ({
            contents:
              "export * as CommonActions from 'expo-source:react-navigation/routers/CommonActions';",
          }));
          builder.onResolve({ filter: /components\/composer-action$/ }, () => ({
            path: "fixture:close-button",
            namespace: "close-button",
          }));
          builder.onLoad({ filter: /.*/, namespace: "close-button" }, () => ({
            contents:
              "import React from 'react';import{qa}from'fixture:state';export function ComposerAction(props){qa.closeProps=props;return React.createElement('button',{disabled:props.disabled,'data-qa-close':true,'aria-label':props.label,onClick:props.onPress},props.name)}",
            loader: "jsx",
          }));
          builder.onResolve({ filter: /^expo-router\/react-navigation$/ }, () => ({
            path: "fixture:sheet-navigation",
            namespace: "sheet-navigation",
          }));
          builder.onLoad({ filter: /.*/, namespace: "sheet-navigation" }, () => ({
            contents:
              "export * as CommonActions from 'expo-source:react-navigation/routers/CommonActions';export {useRoute} from 'expo-source:react-navigation/core/useRoute';",
          }));
          builder.onResolve({ filter: /^\.\/link\/useLoadedNavigation$/ }, ({ importer }) =>
            importer === join(expo, "useFocusEffect.js")
              ? { path: "fixture:loaded-navigation", namespace: "navigation-port" }
              : undefined,
          );
          builder.onLoad({ filter: /.*/, namespace: "navigation-port" }, () => ({
            contents:
              "import {qa} from 'fixture:state';export const useOptionalNavigation=()=>qa.navigation;",
          }));
          builder.onResolve({ filter: /^\.\/react-navigation\/native$/ }, ({ importer }) =>
            importer === join(expo, "useNavigation.js")
              ? { path: "fixture:parent-navigation", namespace: "parent-navigation" }
              : undefined,
          );
          builder.onLoad({ filter: /.*/, namespace: "parent-navigation" }, () => ({
            contents:
              "export {useNavigation} from 'expo-source:react-navigation/core/useNavigation';import {qa} from 'fixture:state';export const useStateForPath=()=>qa.tree.state;",
          }));
          builder.onResolve({ filter: /^expo-router$/ }, () => ({
            path: "fixture:bridge",
            namespace: "fixture",
          }));
          builder.onResolve({ filter: /^react-native$/ }, () => ({
            path: "fixture:native",
            namespace: "fixture",
          }));
          builder.onResolve({ filter: /^expo-constants$/ }, () => ({
            path: "fixture:constants",
            namespace: "constants",
          }));
          builder.onLoad({ filter: /.*/, namespace: "constants" }, () => ({
            contents:
              "export default {executionEnvironment:'standalone',expoConfig:{scheme:'zoen'},linkingUri:'zoen://'};",
          }));
          builder.onResolve({ filter: /^expo\/dom$/ }, () => ({
            path: "fixture:dom",
            namespace: "dom",
          }));
          builder.onLoad({ filter: /.*/, namespace: "dom" }, () => ({
            contents: "export const IS_DOM=false;",
          }));
          builder.onResolve({ filter: /.*/ }, ({ path, importer }) =>
            importer === detail && path.startsWith(".")
              ? path === "./app-request-review"
                ? { path: appRequestReview }
                : ["./deck-version-selection", "./binary-formats"].includes(path)
                  ? { path: join(app, "src/miniapps", `${path}.ts`) }
                  : {
                      path: path === "./view-host" ? "fixture:view" : "fixture:ports",
                      namespace: "fixture",
                    }
              : undefined,
          );
        },
      },
    ],
  });
  const module = { exports: {} };
  new Function("require", "module", "exports", bundled.outputFiles[0].text)(
    (id) => {
      try {
        return appRequire(id);
      } catch (error) {
        error.message = "External " + id + ": " + error.message;
        throw error;
      }
    },
    module,
    module.exports,
  );
  let mounted;
  await React.act(async () => {
    mounted = module.exports.mount(options);
  });
  return {
    ...mounted,
    async act(work) {
      await React.act(async () => {
        await work();
        await new Promise(setImmediate);
      });
    },
    async close() {
      await React.act(async () => mounted.unmount());
      dom.window.close();
      for (const [key, desc] of Object.entries(prior))
        if (desc) Object.defineProperty(globalThis, key, desc);
        else delete globalThis[key];
    },
  };
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function documentPort() {
  const pending = [];
  const calls = [];
  return {
    calls,
    pending,
    controller: {
      flush(options) {
        calls.push(options);
        const wait = deferred();
        pending.push(wait);
        return wait.promise;
      },
    },
  };
}
function current(f) {
  return f.qa.tree.state.routes[f.qa.tree.state.index];
}

export { fixture, deferred, documentPort, current };
