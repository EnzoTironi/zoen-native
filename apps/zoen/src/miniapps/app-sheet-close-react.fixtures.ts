import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import * as React from 'react';
import { createReactTestRoot } from '../core/react-test-root.fixtures.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const app = fileURLToPath(new URL('../..', import.meta.url));
const expo = join(dirname(require.resolve('expo-router/package.json')), 'build');
const chrome = join(app, 'src/miniapps/app-sheet-chrome.tsx');
const detail = join(app, 'src/miniapps/detail-screen.tsx');

const entry = `
import React,{useRef,useState} from 'react';
import{AppSheetChrome}from'production:chrome';
import{MiniappScreen}from'production:detail';
import{useNavigationTreeReducer}from'expo-source:global-state/useNavigationTreeReducer';
import{useNavigationTreeReportEvents}from'expo-source:global-state/useNavigationTreeReportEvents';
import{RemovalPreventionProvider,PreventRemovalProvider,useRoutesWithRemovalPrevented}from'expo-source:global-state/removalPrevention';
import{NavigationContext}from'expo-source:react-navigation/core/NavigationContext';
import{NavigationRouteContext}from'expo-source:react-navigation/core/NavigationProvider';
import{StackRouter}from'expo-source:react-navigation/routers/StackRouter';
import{createImperativeRouter,router}from'expo-source:global-state/router';
import{qa}from'fixture:state';
const stack=(key,names,routes)=>({key,type:'stack',stale:false,routeKeySeq:10,routeNames:names,routes,index:routes.length-1});
function register(state,registry){const router=StackRouter({});registry.set(state.key,{reduce:(current,action)=>router.getStateForAction(current,action,{routeNames:current.routeNames,routeGetIdList:{}}),getStateForRouteFocus:router.getStateForRouteFocus,shouldActionChangeFocus:router.shouldActionChangeFocus});for(const route of state.routes)if(route.state)register(route.state,registry);}
function Harness(){
 const[,render]=useState(0);qa.rerender=()=>render(n=>n+1);
 const prevented=useRoutesWithRemovalPrevented();
 const tree=useNavigationTreeReducer({initialState:qa.initial,registry:qa.registry,routesWithRemovalPrevented:prevented,linking:{getPathFromState:()=>'/fixture'}});
 useNavigationTreeReportEvents(tree.report,tree.consumeReportEvents,{apply(){}});qa.tree=tree;qa.prevented=prevented;
 qa.navigation.getState=()=>tree.state;qa.navigation.canGoBack=()=>tree.state.routes.length>1;
 qa.navigation.dispatch=action=>{qa.dispatched.push(action);tree.handleAction(action)};
 Object.assign(router,createImperativeRouter(intent=>tree.processIntent(intent),()=>{}));
 const current=tree.state.routes[tree.state.index];qa.destination=current.name;
 if(current.name!=='miniapp')return null;
 return <NavigationContext.Provider value={qa.navigation}><NavigationRouteContext.Provider value={qa.routeIdentity}><PreventRemovalProvider routeKey="app-modal"><AppSheetChrome>
 {qa.canvas&&<PreventRemovalProvider routeKey="drawing" emitRemovalEvent={(_key,type,action)=>qa.navigation.emit(type,{data:{action}})}><MiniappScreen/></PreventRemovalProvider>}
 {qa.manage&&<PreventRemovalProvider routeKey="management"><MiniappScreen manage/></PreventRemovalProvider>}
 </AppSheetChrome></PreventRemovalProvider></NavigationRouteContext.Provider></NavigationContext.Provider>;
}
export function create(options){
 const events=new Map();Object.assign(qa,{...options,dispatched:[],focused:true,button:null,escape:null,disposed:0,labels:new Set(),routeIdentity:{key:'app-modal',name:'miniapp'},navigation:{getId(){return''},isFocused:()=>qa.focused,addListener(type,fn){const set=events.get(type)??new Set();events.set(type,set);set.add(fn);return()=>set.delete(fn)},emit(type,event){for(const fn of [...(events.get(type)??[])])fn(event)}}});
 const routes=[];if(qa.canvas)routes.push({key:'drawing',name:'[id]'});if(qa.manage)routes.push({key:'management',name:'[id]/manage'});
 qa.initial=stack('root-stack',['library','chat','miniapp'],[{key:options.origin,name:options.origin},{key:'app-modal',name:'miniapp',state:stack('apps-stack',['[id]','[id]/manage'],routes)}]);qa.registry=new Map();register(qa.initial,qa.registry);
 return{qa,element:<RemovalPreventionProvider><Harness/></RemovalPreventionProvider>};
}`;
const ports = `
import React from'react';import{qa}from'fixture:state';
export const t=key=>key;
export function useClient(){return{store:{}}}
export function useCoreQuery(input){return{data:input.op==='item'?{id:'drawing',title:'Drawing',space_id:'space',app:{app_id:'excalidraw'}}:[{id:'excalidraw',has_view:true,tools:[]}],retry(){}}}
export const useTheme=()=>({colors:{danger:'#b00',secondary:'#888'}});
export const useHome=()=>({items:[],isPinned(){return false}});
export const useDialogs=()=>({});export const useConversationCards=()=>({items:[],isPinned(){return false}});
export const MiniappState=()=>null,MiniappSurfaceControls=()=>null,HomeCardMenu=()=>null,EditorAgentAccess=()=>null,AppPackageReleaseSheet=()=>null;
export const callMiniapp=()=>{throw Error('Unexpected tool')};
export const EmptyState=()=>null,Loading=()=>null,NativeButton=()=>null;
export const Surface=({children})=>React.createElement(React.Fragment,null,children);
`;
const native = `
import React,{useLayoutEffect}from'react';import{qa}from'fixture:state';
export const Platform={OS:'ios'};
export const StyleSheet={absoluteFill:{position:'absolute',top:0,right:0,bottom:0,left:0}};
export function View(props){useLayoutEffect(()=>{if(Object.hasOwn(props,'onAccessibilityEscape'))qa.escape=props.onAccessibilityEscape;},[props]);return React.createElement(React.Fragment,null,props.children)}
export const ScrollView=View;
export function Text({children}){useLayoutEffect(()=>{if(typeof children==='string')qa.labels.add(children);return()=>qa.labels.delete(children)},[children]);return null}
`;
const bridge = `
import React from'react';import{qa}from'fixture:state';
export{router}from'expo-source:global-state/router';
export{usePreventRemove}from'expo-source:react-navigation/core/usePreventRemove';
export{useFocusEffect}from'expo-source:useFocusEffect';
export{useNavigation}from'expo-source:useNavigation';
export const useLocalSearchParams=()=>({id:'drawing'}),usePathname=()=>qa.path;
export function Stack(){return null}Stack.Screen=()=>null;
`;
const view = `
import React,{useEffect}from'react';import{qa}from'fixture:state';import{useAppSheetCloseButton}from'production:context';
export function MiniappView({onDocumentController}){qa.chrome=useAppSheetCloseButton();useEffect(()=>{onDocumentController(qa.controller);return()=>{qa.disposed++;onDocumentController(null)}},[onDocumentController]);return null}
`;

/** Actual Expo reducer/removal hooks and Compiler-transformed production chrome/screen.
 * Native presentation, account reads and document persistence are explicit ports. */
export async function appSheetCloseFixture({
  manage = false,
  canvas = !manage,
  origin = 'library',
}: { manage?: boolean; canvas?: boolean; origin?: string } = {}) {
  const pending: { resolve(): void; reject(error: Error): void }[] = [];
  const flushes: unknown[] = [];
  const controller = {
    flush(options: unknown) {
      flushes.push(options);
      return new Promise<void>((resolve, reject) => pending.push({ resolve, reject }));
    },
  };
  const result = await build({
    stdin: { contents: entry, resolveDir: app, loader: 'jsx' },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    jsx: 'automatic',
    packages: 'external',
    loader: { '.png': 'dataurl' },
    plugins: [
      {
        name: 'app-close-explicit-ports',
        setup(b) {
          b.onResolve({ filter: /^react(?:\/.*)?$|^react-dom(?:\/.*)?$/ }, ({ path }) => ({
            path,
            external: true,
          }));
          b.onLoad({ filter: /\/(app-sheet-chrome|detail-screen)\.tsx$/ }, ({ path }) => ({
            contents: transformSync(readFileSync(path, 'utf8'), {
              filename: path,
              babelrc: false,
              configFile: false,
              presets: [[require.resolve('babel-preset-expo'), { jsxRuntime: 'automatic' }]],
              plugins: [[require.resolve('babel-plugin-react-compiler'), { target: '19' }]],
              caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
            }).code,
            loader: 'js',
          }));
          b.onResolve({ filter: /^expo-source:/ }, ({ path }) => ({
            path: join(expo, path.slice(12) + '.js'),
          }));
          b.onResolve({ filter: /^production:/ }, ({ path }) => ({
            path:
              path === 'production:chrome'
                ? chrome
                : path === 'production:detail'
                  ? detail
                  : join(app, 'src/miniapps/app-sheet-context.ts'),
          }));
          b.onResolve({ filter: /^fixture:/ }, ({ path }) => ({ path, namespace: 'fixture' }));
          b.onLoad({ filter: /.*/, namespace: 'fixture' }, ({ path }) => ({
            contents:
              path === 'fixture:state'
                ? 'export const qa={};'
                : path === 'fixture:bridge'
                  ? bridge
                  : path === 'fixture:native'
                    ? native
                    : path === 'fixture:view'
                      ? view
                      : ports,
            loader: 'jsx',
          }));
          b.onResolve({ filter: /^expo-router$/ }, () => ({
            path: 'fixture:bridge',
            namespace: 'fixture',
          }));
          b.onResolve({ filter: /^react-native$/ }, () => ({
            path: 'fixture:native',
            namespace: 'fixture',
          }));
          b.onResolve({ filter: /^react-native-safe-area-context$/ }, () => ({
            path: 'safe',
            namespace: 'safe',
          }));
          b.onLoad({ filter: /.*/, namespace: 'safe' }, () => ({
            contents: 'export const useSafeAreaInsets=()=>({top:24,right:0,bottom:16,left:0});',
          }));
          b.onResolve({ filter: /^expo-router\/react-navigation$/ }, () => ({
            path: 'nav',
            namespace: 'nav',
          }));
          b.onLoad({ filter: /.*/, namespace: 'nav' }, () => ({
            contents:
              "export * as CommonActions from'expo-source:react-navigation/routers/CommonActions';export{useRoute}from'expo-source:react-navigation/core/useRoute';",
          }));
          b.onResolve({ filter: /components\/composer-action$/ }, () => ({
            path: 'close',
            namespace: 'close',
          }));
          b.onLoad({ filter: /.*/, namespace: 'close' }, () => ({
            contents:
              "import React from'react';import{qa}from'fixture:state';export function ComposerAction(props){React.useLayoutEffect(()=>{qa.button=props;return()=>{if(qa.button===props)qa.button=null}},[props]);return null;}",
          }));
          b.onResolve({ filter: /^\.\/link\/useLoadedNavigation$/ }, ({ importer }) =>
            importer === join(expo, 'useFocusEffect.js')
              ? { path: 'loaded', namespace: 'loaded' }
              : undefined,
          );
          b.onLoad({ filter: /.*/, namespace: 'loaded' }, () => ({
            contents:
              "import{qa}from'fixture:state';export const useOptionalNavigation=()=>qa.navigation;",
          }));
          b.onResolve({ filter: /^\.\/react-navigation\/native$/ }, ({ importer }) =>
            importer === join(expo, 'useNavigation.js')
              ? { path: 'parent', namespace: 'parent' }
              : undefined,
          );
          b.onLoad({ filter: /.*/, namespace: 'parent' }, () => ({
            contents:
              "export{useNavigation}from'expo-source:react-navigation/core/useNavigation';import{qa}from'fixture:state';export const useStateForPath=()=>qa.tree.state;",
          }));
          b.onResolve({ filter: /^expo-constants$/ }, () => ({
            path: 'constants',
            namespace: 'constants',
          }));
          b.onLoad({ filter: /.*/, namespace: 'constants' }, () => ({
            contents:
              "export default{executionEnvironment:'standalone',expoConfig:{scheme:'zoen'},linkingUri:'zoen://'};",
          }));
          b.onResolve({ filter: /^expo\/dom$/ }, () => ({ path: 'dom', namespace: 'dom' }));
          b.onLoad({ filter: /.*/, namespace: 'dom' }, () => ({
            contents: 'export const IS_DOM=false;',
          }));
          b.onResolve({ filter: /.*/ }, ({ path, importer }) => {
            if (importer === chrome && path === '../i18n/index.ts')
              return { path: 'fixture:ports', namespace: 'fixture' };
            if (importer === detail && path === './app-request-review')
              return { path: join(app, 'src/miniapps/app-request-review.ts') };
            if (
              importer === detail &&
              path.startsWith('.') &&
              !['./deck-version-selection', './binary-formats'].includes(path)
            )
              return {
                path: path === './view-host' ? 'fixture:view' : 'fixture:ports',
                namespace: 'fixture',
              };
            return undefined;
          });
        },
      },
    ],
  });
  const module = {
    exports: {} as {
      create(options: Record<string, unknown>): { qa: any; element: React.ReactNode };
    },
  };
  new Function('require', 'module', 'exports', result.outputFiles[0]!.text)(
    require,
    module,
    module.exports,
  );
  const { qa, element } = module.exports.create({
    manage,
    canvas,
    origin,
    controller,
    path: `/miniapp/drawing${manage ? '/manage' : ''}`,
  });
  const root = createReactTestRoot();
  await root.render(element);
  assert.deepEqual(root.errors, []);
  return {
    qa,
    flushes,
    pending,
    errors: root.errors,
    act: root.act,
    focus(value: boolean) {
      qa.focused = value;
      qa.navigation.emit(value ? 'focus' : 'blur', {});
    },
    path(value: string) {
      qa.path = value;
      qa.rerender();
    },
    current: () => qa.tree.state.routes[qa.tree.state.index],
    close: root.close,
  };
}
