import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { App } from '@modelcontextprotocol/ext-apps';
import type { Transport } from '@modelcontextprotocol/client';
import { createReactTestRoot } from '../core/react-test-root.fixtures.ts';
import { ClientStore } from '../core/store.ts';
import { Driver, draftSession, tick } from '../core/store.fixtures.ts';
import { t, locale } from '../i18n/index.ts';
import * as lifetime from './lifetime.ts';
import * as protocol from './protocol.ts';
import * as session from './remote-session.ts';
import * as opening from './remote-opening.ts';
import * as grants from './remote-grants.ts';
import * as sandbox from './remote-sandbox.ts';
import * as presentation from './excalidraw-presentation.ts';
import type { MiniappFrameProps, MiniappFrameHandle } from './frame-types.ts';
import { fixture, uri } from './remote-session.fixture.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('./remote-view.tsx', import.meta.url);
const compiled = transformSync(readFileSync(process.env.ZOEN_REMOTE_VIEW_SOURCE ?? file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.match(compiled, /react\/compiler-runtime/);

export function externalLaunch(): opening.RemoteAppLaunch {
  const html = '<!doctype html><p>Provider app</p>';
  return {
    binding: {
      id: 'external-editor',
      name: 'External editor',
      endpoint: 'https://apps.example.com/mcp',
      release: '2026.10.25',
      launchTool: 'paint',
      resource: {
        uri,
        sha256: createHash('sha256').update(html).digest('hex'),
        bytes: Buffer.byteLength(html),
        csp: {
          connectDomains: ['https://api.example.com'],
          resourceDomains: ['https://assets.example.com'],
          frameDomains: [],
          baseUriDomains: [],
        },
        permissions: { clipboardWrite: {} },
      },
    },
    arguments: { document: 'synthetic-host-launch' },
  };
}

type Control = { testID: string; disabled?: boolean; onPress(): void };
type Frame = MiniappFrameProps;
/** Real compiled screen, React/Expo focus, Store, remote SDK and AppBridge.
 * Native controls/frame are ports: this proves composition, not native pixels. */
export async function remoteViewFixture(options: Parameters<typeof fixture>[0] = {}) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const provider = fixture({ endpoint: externalLaunch().binding.endpoint, ...options });
  driver.opening = async () => structuredClone(snapshot);
  const coreCalls: string[] = [];
  driver.operation = async (command) => {
    coreCalls.push(command.op);
    if (command.op === 'snapshot') return structuredClone(snapshot);
    throw new Error(`Unexpected external App core command: ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  let focused = true;
  const listeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', callback: () => void) {
      listeners[name].add(callback);
      return () => listeners[name].delete(callback);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as { useFocusEffect: unknown } };
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
        throw new Error(name);
      },
    },
  );
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw new Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const buttons = new Map<string, Control>(),
    frameHistory: Frame[] = [],
    sent: unknown[] = [];
  const confirmations: unknown[] = [],
    alerts: unknown[] = [];
  let frame: Frame | null = null,
    disposals = 0,
    appTransport: Transport | null = null;
  let decision: Promise<boolean> | boolean = true;
  const dialogs = {
    confirm: async (input: unknown) => {
      confirmations.push(input);
      return decision;
    },
  };
  function NativeButton(props: Control) {
    React.useLayoutEffect(() => {
      buttons.set(props.testID, props);
      return () => {
        if (buttons.get(props.testID) === props) buttons.delete(props.testID);
      };
    }, [props]);
    return null;
  }
  const MiniappFrame = React.forwardRef<MiniappFrameHandle, Frame>(function FramePort(props, ref) {
    React.useImperativeHandle(
      ref,
      () => ({
        send(message) {
          sent.push(message);
          appTransport?.onmessage?.(message as never);
        },
        dispose() {
          disposals++;
        },
      }),
      [],
    );
    React.useLayoutEffect(() => {
      frame = props;
      frameHistory.push(props);
      return () => {
        if (frame === props) frame = null;
      };
    }, [props]);
    return null;
  });
  const colors = {
    background: '#000000',
    text: '#ffffff',
    secondary: '#a0a0a0',
    danger: '#ff0000',
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      View: ({ children }: { children: React.ReactNode }) => children,
      Text: (props: { accessibilityRole?: string; children?: unknown }) => {
        if (props.accessibilityRole === 'alert') alerts.push(props.children);
        return null;
      },
      Linking: { openURL: async () => {} },
    },
    'expo-router': {
      useFocusEffect: focus.exports.useFocusEffect,
      Stack: { Screen: () => null },
      usePreventRemove: () => {},
    },
    'expo-router/react-navigation': { useHeaderHeight: () => 0 },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ top: 0, bottom: 0, left: 0, right: 0 }),
    },
    'expo/fetch': { fetch: provider.fetcher },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../components/primitives': {
      NativeButton,
      Loading: () => null,
      Surface: ({ children }: { children: React.ReactNode }) => children,
      EmptyState: (props: { detail?: string }) => {
        if (props.detail) alerts.push(props.detail);
        return null;
      },
    },
    '../components/dialogs': { useDialogs: () => dialogs },
    '../theme': { useTheme: () => ({ colors, scheme: 'dark' }) },
    '../i18n/index.ts': { t, locale },
    './frame': { MiniappFrame },
    './lifetime': lifetime,
    './protocol': protocol,
    './remote-grants': grants,
    './remote-session': session,
    './remote-opening': opening,
    './remote-sandbox': sandbox,
    './excalidraw-presentation': presentation,
  };
  const module = {
    exports: {} as { RemoteAppScreen: React.ComponentType<opening.RemoteAppScreenProps> },
  };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    Error,
    URL,
    Intl,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const apps: App[] = [];
  return {
    store,
    provider,
    coreCalls,
    frameHistory,
    sent,
    alerts,
    confirmations,
    errors: root.errors,
    frame: () => frame,
    disposals: () => disposals,
    button: () => buttons.get('start-connected-app') ?? buttons.get('start-excalidraw-drawing'),
    act: root.act,
    decide(next: Promise<boolean> | boolean) {
      decision = next;
    },
    render: (props: opening.RemoteAppScreenProps = { launch: externalLaunch() }) =>
      root.render(React.createElement(module.exports.RemoteAppScreen, props)),
    async settle() {
      await root.act(tick);
    },
    async blur() {
      await root.act(() => {
        focused = false;
        for (const fn of listeners.blur) fn();
      });
    },
    async connectApp() {
      const app = new App({ name: 'external-ui', version: '1' }, {}, { autoResize: false });
      app.onteardown = async () => ({});
      appTransport = {
        start: async () => {},
        close: async () => {},
        send: async (message) => {
          frame?.onMessage(JSON.stringify(message));
        },
      };
      await app.connect(appTransport);
      apps.push(app);
      return app;
    },
    async close() {
      await root.close();
      for (const app of apps) await app.close();
      await store.close();
      await provider.session.close();
    },
  };
}
