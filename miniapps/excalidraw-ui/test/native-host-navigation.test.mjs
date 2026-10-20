import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

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
    dispatched: [], focused: true, path: '/miniapp/synthetic-drawing', routeIdentity: {key:'app-modal',name:'miniapp'},
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
export const MiniappState=()=>null,MiniappSurfaceControls=()=>null,HomeCardMenu=()=>null;
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
              ? {
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

for (const origin of ["home", "chat"])
  test(`Expo default Back waits for actual screen flush and returns to ${origin}`, async () => {
    const doc = documentPort();
    const f = await fixture("android", { controller: doc.controller, origin });
    try {
      assert.equal(f.qa.backHandlers.size, 1);
      assert.ok(f.qa.prevented.has("drawing"));
      assert.ok(
        f.node.querySelector("[data-qa-local-safe-provider] [data-qa-editor]"),
        "actual App layout places editor inside region-local native provider",
      );
      await f.act(() => assert.equal(f.back(), true));
      assert.equal(current(f).name, "miniapp");
      assert.deepEqual(doc.calls, [{ retire: true }]);
      assert.equal(f.qa.disposedDocuments, 0);
      await f.act(() => assert.equal(f.back(), true));
      assert.equal(doc.calls.length, 1, "duplicate Back shares actual screen navigation lock");
      await f.act(() => doc.pending[0].resolve());
      assert.equal(current(f).name, origin);
      assert.equal(f.qa.disposedDocuments, 1);
      assert.equal(f.qa.nativeFallbacks, 0);
      assert.equal(f.qa.backHandlers.size, 1, "root listener survives App dismissal");
    } finally {
      await f.close();
    }
  });
test("actual failed flush retains App, exposes close error, and allows successful Back retry", async () => {
  const doc = documentPort();
  const f = await fixture("android", { controller: doc.controller });
  try {
    await f.act(() => f.back());
    await f.act(() => doc.pending[0].reject(Error("Synthetic save failure")));
    assert.equal(current(f).name, "miniapp");
    assert.equal(f.qa.disposedDocuments, 0);
    assert.ok(f.qa.prevented.has("drawing"));
    assert.match(f.node.textContent, /drawing.closeFailed/);
    await f.act(() => f.back());
    assert.equal(doc.calls.length, 2);
    assert.equal(current(f).name, "miniapp");
    await f.act(() => doc.pending[1].resolve());
    assert.equal(current(f).name, "home");
    assert.equal(f.qa.nativeFallbacks, 0);
  } finally {
    await f.close();
  }
});
test("Expo default Back closes nested management before flushing the App", async () => {
  const doc = documentPort();
  const f = await fixture("android", { controller: doc.controller, management: true });
  try {
    await f.act(() => f.back());
    assert.equal(current(f).name, "miniapp");
    assert.equal(current(f).state.index, 0);
    assert.equal(doc.calls.length, 0);
    await f.act(() => f.back());
    assert.equal(doc.calls.length, 1);
    assert.equal(current(f).name, "miniapp");
    await f.act(() => doc.pending[0].resolve());
    assert.equal(current(f).name, "home");
  } finally {
    await f.close();
  }
});
test("root Back falls through to native after the App has closed", async () => {
  const f = await fixture("android", { loading: true });
  try {
    await f.act(() => assert.equal(f.back(), true));
    assert.equal(current(f).name, "home");
    assert.equal(f.back(), false);
    assert.equal(f.qa.nativeFallbacks, 1);
  } finally {
    await f.close();
    assert.equal(f.qa.backHandlers.size, 0, "root listener cleans up on unmount");
  }
});
test("Web uses Expo's no-op Back listener", async () => {
  const doc = documentPort();
  const f = await fixture("web", { controller: doc.controller });
  try {
    assert.equal(f.qa.backHandlers.size, 0);
    assert.equal(f.back(), false);
    assert.equal(doc.calls.length, 0);
  } finally {
    await f.close();
  }
});
test("App stack Back also consumes while its document is still loading", async () => {
  const f = await fixture("android", { loading: true });
  try {
    assert.ok(f.node.querySelector("[data-qa-loading]"));
    await f.act(() => assert.equal(f.back(), true));
    assert.equal(current(f).name, "home");
  } finally {
    await f.close();
  }
});
test("default Back defers to native while the navigation ref is unavailable", async () => {
  const doc = documentPort();
  const f = await fixture("android", { controller: doc.controller });
  try {
    f.qa.rootRef.current = null;
    assert.equal(f.back(), false);
    assert.equal(f.qa.nativeFallbacks, 1);
    assert.equal(current(f).name, "miniapp");
    assert.equal(doc.calls.length, 0);
  } finally {
    await f.close();
  }
});

function pressClose(f) {
  const button = f.node.querySelector("[data-qa-close]");
  assert.ok(button, "actual App layout mounts its native close component");
  button.click();
}
for (const platform of ["ios", "android"]) {
  test(`${platform} floating close uses actual root removal guard and preserves full App safe insets`, async () => {
    const doc = documentPort();
    const f = await fixture(platform, { controller: doc.controller, origin: "chat" });
    try {
      assert.deepEqual(
        f.qa.editorInsets,
        { top: 24, right: 0, bottom: 16, left: 0 },
        "no global 48-point row",
      );
      assert.deepEqual(f.qa.editorChrome, { top: 32, right: 12, width: 44, height: 44 });
      assert.equal(f.qa.closeProps.name, "xmark");
      assert.equal(f.qa.closeProps.size, 44);
      assert.equal(f.qa.closeProps.label, "common.close");
      await f.act(() => pressClose(f));
      assert.equal(current(f).name, "miniapp");
      assert.equal(f.qa.disposedDocuments, 0);
      assert.deepEqual(doc.calls, [{ retire: true }]);
      assert.deepEqual(f.qa.dispatched[0], {
        type: "GO_BACK",
        source: "app-modal",
        target: "root-stack",
      });
      await f.act(() => pressClose(f));
      assert.equal(doc.calls.length, 1, "repeated close shares descendant save lock");
      await f.act(() => doc.pending[0].resolve());
      assert.equal(current(f).name, "chat");
      assert.equal(f.qa.disposedDocuments, 1);
    } finally {
      await f.close();
    }
  });
  test(`${platform} failed close retains document and a new explicit X retries the real flush`, async () => {
    const doc = documentPort();
    const f = await fixture(platform, { controller: doc.controller });
    try {
      await f.act(() => pressClose(f));
      await f.act(() => doc.pending[0].reject(Error("Synthetic durable save failure")));
      assert.equal(current(f).name, "miniapp");
      assert.equal(f.qa.disposedDocuments, 0);
      assert.ok(f.qa.prevented.has("drawing"));
      assert.match(f.node.textContent, /drawing.closeFailed/);
      await f.act(() => pressClose(f));
      assert.equal(doc.calls.length, 2);
      await f.act(() => doc.pending[1].resolve());
      assert.equal(current(f).name, "home");
      assert.equal(f.qa.disposedDocuments, 1);
    } finally {
      await f.close();
    }
  });
}
test("a retained close callback cannot close a refocused or replaced App scope", async () => {
  const doc = documentPort();
  const f = await fixture("ios", { controller: doc.controller });
  try {
    const beforeBlur = f.qa.closeProps.onPress;
    await f.act(() => {
      f.focus(false);
      f.focus(true);
    });
    await f.act(() => beforeBlur());
    assert.deepEqual(f.qa.dispatched, []);
    const beforePath = f.qa.closeProps.onPress;
    await f.act(() => f.path("/miniapp/different-document"));
    await f.act(() => beforePath());
    assert.deepEqual(f.qa.dispatched, []);
    assert.equal(doc.calls.length, 0);
    await f.act(() => pressClose(f));
    assert.equal(doc.calls.length, 1);
    await f.act(() => doc.pending[0].resolve());
    assert.equal(current(f).name, "home");
  } finally {
    await f.close();
  }
});
test("floating close refuses missing state, root mismatch, unfocused route and absent back destination", async () => {
  const doc = documentPort();
  const f = await fixture("ios", { controller: doc.controller });
  try {
    const read = f.qa.navigation.getState,
      canBack = f.qa.navigation.canGoBack;
    f.qa.navigation.getState = () => undefined;
    await f.act(() => pressClose(f));
    f.qa.navigation.getState = read;
    const key = f.qa.routeIdentity.key;
    f.qa.routeIdentity.key = "other-modal";
    await f.act(() => pressClose(f));
    f.qa.routeIdentity.key = key;
    await f.act(() => f.focus(false));
    await f.act(() => pressClose(f));
    await f.act(() => f.focus(true));
    f.qa.navigation.canGoBack = () => false;
    await f.act(() => pressClose(f));
    f.qa.navigation.canGoBack = canBack;
    assert.deepEqual(f.qa.dispatched, []);
    assert.equal(doc.calls.length, 0);
    assert.equal(current(f).name, "miniapp");
  } finally {
    await f.close();
  }
});
test("floating close from management targets the root App after its existing descendant flush", async () => {
  const doc = documentPort();
  const f = await fixture("ios", { controller: doc.controller, management: true });
  try {
    await f.act(() => pressClose(f));
    assert.equal(current(f).state.index, 1);
    assert.deepEqual(doc.calls, [{ retire: true }]);
    await f.act(() => doc.pending[0].resolve());
    assert.equal(current(f).name, "home");
    assert.equal(f.qa.disposedDocuments, 1);
  } finally {
    await f.close();
  }
});
test("Web App layout has no native X or host corner exclusion", async () => {
  const f = await fixture("web", { controller: documentPort().controller });
  try {
    assert.equal(f.node.querySelector("[data-qa-close]"), null);
    assert.equal(f.qa.editorChrome, null);
    assert.deepEqual(f.qa.editorInsets, { top: 24, right: 0, bottom: 16, left: 0 });
  } finally {
    await f.close();
  }
});
