import assert from 'node:assert/strict';
import * as messageDraft from './message-draft.ts';
import * as mediaReply from '../media/media-reply.ts';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { NormalMessageDrafts } from './normal-message-drafts.ts';
import type { ClientState } from './store.ts';
import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import { LocationSelection } from '../media/location-selection.ts';
import * as actions from '../media/attachment-route-actions.ts';
import * as content from '../media/attachment-content.ts';
import type { StaticLocation } from '../media/types.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const compile = (path: URL) =>
  transformSync(readFileSync(path, 'utf8'), {
    filename: path.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
const routeCode = compile(new URL('../media/attachment-route.tsx', import.meta.url));
const pickerCode = compile(new URL('../media/location-picker.tsx', import.meta.url));
const focusCode = readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8');
const loadedCode = readFileSync(
  require.resolve('expo-router/build/link/useLoadedNavigation.js'),
  'utf8',
);
type Button = { label: string; disabled?: boolean; onPress(): void };
type MapProps = {
  binding: string;
  editable: boolean;
  point?: StaticLocation;
  onPointChange(point: StaticLocation): void;
};
type FieldProps = { label: string; editable: boolean; onChangeText(text: string): void };

export function locationReactFixture() {
  const root = createReactTestRoot();
  let state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'owner', device_id: 'device', unlocked: true, setup_pending: false },
      me: { id: 'owner' },
      conversations: [{ id: 'space', members: [{ id: 'owner' }] }],
    },
  } as unknown as ClientState;
  let stages = 0,
    gps = 0;
  let foreground = 'active',
    focused = true,
    uuid = 0,
    backs = 0,
    sends = 0;
  const appListeners = new Set<(value: string) => void>();
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', callback: () => void) {
      focusListeners[name].add(callback);
      return () => focusListeners[name].delete(callback);
    },
  };
  // Use both installed Expo hooks and the real React lifecycle, instead of
  // inventing navigation readiness/effect order or flattening child hooks.
  const loaded = { exports: {} };
  const navigationContext = React.createContext({});
  runInNewContext(loadedCode, {
    module: loaded,
    exports: loaded.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === '../react-navigation/native')
        return {
          useNavigation: () => navigation,
          NavigationContainerRefContext: navigationContext,
        };
      throw Error(name);
    },
  });
  const focus = {
    exports: {} as { useFocusEffect: (callback: () => void | (() => void)) => void },
  };
  runInNewContext(focusCode, {
    module: focus,
    exports: focus.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const controller = new NormalMessageDrafts({
    load: async () => ({ revision: '0', text: 'Retained caption', reply_to: null }),
    save: async (_, revision, text, reply_to) => ({ revision, text, reply_to }),
    send: async () => {
      sends++;
      throw Error('not admitted by this test');
    },
  });
  const normal = {
    controller,
    scope: {
      owner: { mode: 'account' as const, identityId: 'owner', deviceId: 'device', sessionEpoch: 1 },
      spaceId: 'space',
      threadRootId: null,
    },
  };
  const mediaChannel = {};
  const searches: string[] = [],
    cancels: string[] = [];
  let searchGate: Promise<readonly StaticLocation[]> | null = null;
  const driver = {
    searchLocations: async (_: string, query: string) => {
      searches.push(query);
      return searchGate ?? [{ longitude: 4, latitude: 3, label: 'Synthetic place' }];
    },
    cancelLocationSearch: (binding: string) => {
      cancels.push(binding);
    },
    readLocation: async () => {
      throw Error('not a received file');
    },
  };
  const sameOwner = () =>
    homeOwnerFor(state)?.identityId === 'owner' && homeSpacesFor(state).has('space');
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    driver: { media: driver },
    messageDrafts: {
      channel: () => normal,
      isCurrent: (value: unknown) => value === normal && sameOwner(),
    },
    mediaDrafts: {
      channel: () => mediaChannel,
      isCurrent: (value: unknown) => value === mediaChannel && sameOwner(),
    },
  };
  const buttons = new Map<string, Button>(),
    fields = new Map<string, FieldProps>();
  let map: MapProps | undefined;
  const useCapture = <T>(
    read: () => T,
    publish: (props: T) => void,
    release: (props: T) => void,
  ) => {
    const props = read();
    React.useLayoutEffect(() => {
      publish(props);
      return () => release(props);
    }, [props, publish, release]);
    return null;
  };
  const children = ({ children }: { children?: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children);
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      View: children,
      Text: () => null,
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
    'react-native-keyboard-controller': { KeyboardAwareScrollView: children },
    'expo-router': {
      useFocusEffect: focus.exports.useFocusEffect,
      usePreventRemove: () => () => {},
      router: {
        back: () => {
          backs++;
        },
      },
      Stack: {
        Screen: ({ options }: { options: { headerLeft?: () => React.ReactNode } }) =>
          options.headerLeft?.() ?? null,
      },
    },
    'expo-crypto': { randomUUID: () => `binding-${++uuid}` },
    '../components/primitives': {
      Field: (props: FieldProps) =>
        useCapture(
          () => props,
          (p) => fields.set(p.label, p),
          (p) => {
            if (fields.get(p.label) === p) fields.delete(p.label);
          },
        ),
      NativeButton: (props: Button) =>
        useCapture(
          () => props,
          (p) => buttons.set(p.label, p),
          (p) => {
            if (buttons.get(p.label) === p) buttons.delete(p.label);
          },
        ),
      Loading: () => null,
    },
    '../components/dialogs': { useDialogs: () => ({ confirm: async () => true }) },
    '../core/provider': { useClient: () => ({ ...state, store }) },
    '../home/access': { homeOwnerFor, homeSpacesFor },
    '../i18n': { t: (name: string) => name },
    '../theme': { useTheme: () => ({ colors: {} }) },
    './location-map': {
      LocationMap: (props: MapProps) =>
        useCapture(
          () => props,
          (p) => {
            map = p;
          },
          (p) => {
            if (map === p) map = undefined;
          },
        ),
    },
    './location-selection': { LocationSelection },
    './attachment-route-actions': actions,
    './media-reply': mediaReply,
    './attachment-content': content,
    './attachment-route-platform': {
      chooseStaticPoint: async () => {
        gps++;
        return null;
      },
    },
    './cache-work': {
      sensitiveCacheWork: {
        run: () => {
          stages++;
          throw Error('No attachment Add authorized in this lifecycle test');
        },
      },
    },
    './article-editor': { ArticleEditor: () => null },
    '../core/message-draft': messageDraft,
    './poll-attachment-editor': { PollAttachmentEditor: () => null },
  };
  const picker = { exports: {} };
  const resolve = (name: string): unknown => {
    if (name.startsWith('@babel/runtime/helpers/')) return require(name);
    assert.ok(name in ports, name);
    return ports[name];
  };
  runInNewContext(pickerCode, { module: picker, exports: picker.exports, require: resolve });
  ports['./location-picker'] = picker.exports;
  const route = { exports: {} as { AttachmentRoute: React.ComponentType<any> } };
  runInNewContext(routeCode, { module: route, exports: route.exports, require: resolve });
  const destination = {
    id: 'space',
    thread: null,
    identity: 'owner',
    device: 'device',
    epoch: '1',
  };
  const render = () =>
    root.render(
      React.createElement(route.exports.AttachmentRoute, { kind: 'location', destination }),
    );
  return {
    warmDraft: () => controller.activate(normal.scope),
    render,
    act: root.act,
    close: root.close,
    errors: root.errors,
    searches,
    cancels,
    get map() {
      return map;
    },
    button(label: string) {
      const value = buttons.get(label);
      assert.ok(value, label);
      return value;
    },
    field(label: string) {
      const value = fields.get(label);
      assert.ok(value, label);
      return value;
    },
    counts: () => ({ backs, sends }),
    effectCounts: () => ({ stages, gps }),
    hasButton: (label: string) => buttons.has(label),
    searchGate: (value: Promise<readonly StaticLocation[]>) => {
      searchGate = value;
    },
    appState(next: string) {
      foreground = next;
      for (const callback of appListeners) callback(next);
    },
    focus(next: boolean) {
      focused = next;
      for (const callback of focusListeners[next ? 'focus' : 'blur']) callback();
    },
    revoke() {
      state = { ...state, snapshot: { ...state.snapshot!, conversations: [] } };
    },
    replaceOwner() {
      state = {
        ...state,
        snapshot: {
          ...state.snapshot!,
          account: { ...state.snapshot!.account!, identity_id: 'other' },
          me: { ...state.snapshot!.me!, id: 'other' },
        },
      };
    },
  };
}
