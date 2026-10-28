import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { build } from 'esbuild';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, deferred } from './store.fixtures.ts';
import { inboxFixture } from './inbox-actions.fixtures.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { home } from '../i18n/catalogs/home.ts';
import { common } from '../i18n/catalogs/common.ts';
import { settings } from '../i18n/catalogs/settings.ts';
import { inboxActions } from '../i18n/catalogs/inbox-actions.ts';
import { telegramPresentation } from '../theme/telegram.ts';

const require = createRequire(import.meta.url),
  { transformSync } = require('@babel/core');
const compiled = await build({
  entryPoints: [new URL('../app/(tabs)/(chats)/chats.tsx', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'native-inbox-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => {
          if (
            [
              'react',
              'react/jsx-runtime',
              'react/compiler-runtime',
              'react-native',
              'expo-router',
              '@shopify/flash-list',
              'expo-haptics',
              'react-native-reanimated',
              'react-native-gesture-handler/ReanimatedSwipeable',
              '@expo/ui/community/menu',
              'react-native-safe-area-context',
              'react-native-enriched-markdown',
            ].includes(path)
          )
            return { path, external: true };
          const aliases: Record<string, string> = {
            '/core/provider': 'client',
            '/home/provider': 'home',
            '/theme': 'theme',
            '/components/primitives': 'primitives',
            '/profile/profile-avatar': 'avatar',
            '/components/message-receipt': 'receipt',
            '/components/chats-actions': 'chatsActions',
            '/miniapps/rail': 'rail',
            '/miniapps/card-edit-controls': 'cardControls',
          };
          if (/\/i18n(?:\/index.ts)?$/.test(path)) return { path: 'fixture:i18n', external: true };
          for (const [ending, name] of Object.entries(aliases))
            if (path.endsWith(ending)) return { path: `fixture:${name}`, external: true };
        });
        builder.onLoad({ filter: /\.(tsx|ts)$/ }, ({ path }) => ({
          contents: transformSync(readFileSync(path, 'utf8'), {
            filename: path,
            babelrc: false,
            configFile: false,
            presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
            plugins: [['babel-plugin-react-compiler', { target: '19' }]],
            caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
          }).code,
          loader: 'js',
        }));
      },
    },
  ],
});

export async function inboxUi(locale: 'en' | 'pt-BR' = 'en') {
  const root = createReactTestRoot(),
    core = inboxFixture(),
    driver = new Driver();
  const calls: any[] = [],
    routes: unknown[] = [],
    haptics: string[] = [];
  const projections: { markdown: string; result: ReturnType<typeof deferred<string>> }[] = [];
  let sequence = 0,
    layout = core.canonical,
    snapshot = async () => structuredClone(core.snapshot);
  driver.opening = async () => structuredClone(core.snapshot);
  driver.operation = async (command: any) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return snapshot();
    if (command.op === 'new_mutation_id') return `ui-intent-${++sequence}`;
    if (command.op === 'chat_layout') return layout(command.input);
    if (command.op === 'set_chat_mute') {
      core.chat.muted = command.mode.kind !== 'off';
      core.chat.muted_until_ms = core.chat.muted ? '9223372036854775807' : null;
      return;
    }
    if (command.op === 'mark_read') {
      core.chat.unread = 0;
      core.chat.manual_unread = false;
      return;
    }
    throw Error(`Unexpected operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const controls = new Map<string, any>(),
    swipes = new Map<string, any>();
  const children = (props: any) => React.createElement(React.Fragment, null, props.children);
  const Capture = (props: any) => {
    const key = props.testID ?? props.label;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (controls.get(key) === props) controls.delete(key);
      };
    }, [key, props]);
    return children(props);
  };
  const CaptureText = (props: any) => {
    React.useLayoutEffect(() => {
      if (props.testID) controls.set(props.testID, props);
      return () => {
        if (controls.get(props.testID) === props) controls.delete(props.testID);
      };
    }, [props]);
    return null;
  };
  const Swipeable = ({ ref, children: content, ...props }: any) => {
    const [edge, setEdge] = React.useState<string | null>(null);
    const methods = React.useMemo(
      () => ({ close: () => setEdge(null), reset: () => setEdge(null) }),
      [],
    );
    React.useImperativeHandle(ref, () => methods, [methods]);
    React.useLayoutEffect(() => {
      swipes.set(props.testID, {
        props,
        reveal: (side: string) => {
          props.onSwipeableWillOpen?.();
          setEdge(side);
        },
      });
      return () => {
        swipes.delete(props.testID);
      };
    }, [props]);
    const translation = { get: () => 0, set: () => {} };
    return React.createElement(
      React.Fragment,
      null,
      content,
      edge === 'leading'
        ? props.renderLeftActions({}, translation, methods)
        : edge === 'trailing'
          ? props.renderRightActions({}, translation, methods)
          : null,
    );
  };
  const list = (props: any) =>
    React.createElement(
      React.Fragment,
      null,
      props.ListHeaderComponent,
      props.data.map((item: any) =>
        React.createElement(
          React.Fragment,
          { key: props.keyExtractor(item) },
          props.renderItem({ item }),
        ),
      ),
      props.ListFooterComponent,
    );
  let focused = true,
    foreground = 'active';
  const focusListeners = new Set<(v: boolean) => void>(),
    appListeners = new Set<(v: string) => void>();
  function useFocusEffect(effect: () => void | (() => void)) {
    React.useEffect(() => {
      let cleanup = focused ? effect() : undefined;
      const changed = (value: boolean) => {
        cleanup?.();
        cleanup = value ? effect() : undefined;
      };
      focusListeners.add(changed);
      return () => {
        cleanup?.();
        focusListeners.delete(changed);
      };
    }, [effect]);
  }
  const localization = createLocalization({ chat, common, inboxActions, home, settings }, [locale]);
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    global: { Error },
    setTimeout,
    clearTimeout,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name === 'react') return React;
      if (
        name === 'react/jsx-runtime' ||
        name === 'react/compiler-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'react-native')
        return {
          View: Capture,
          Text: CaptureText,
          Pressable: Capture,
          ScrollView: children,
          StyleSheet: { hairlineWidth: 0.5 },
          AppState: {
            get currentState() {
              return foreground;
            },
            addEventListener: (_: string, fn: (v: string) => void) => {
              appListeners.add(fn);
              return { remove: () => appListeners.delete(fn) };
            },
          },
        };
      if (name === 'expo-router')
        return {
          Stack: { Screen: () => null },
          router: { push: (route: unknown) => routes.push(route) },
          useFocusEffect,
        };
      if (name === 'react-native-enriched-markdown')
        return {
          markdownToPlainText(markdown: string) {
            const result = deferred<string>();
            projections.push({ markdown, result });
            return result.promise;
          },
        };
      if (name === '@shopify/flash-list') return { FlashList: list };
      if (name === 'react-native-safe-area-context')
        return { useSafeAreaInsets: () => ({ bottom: 34 }) };
      if (name === 'expo-haptics')
        return {
          selectionAsync: async () => {
            haptics.push('selection');
          },
        };
      if (name === 'react-native-gesture-handler/ReanimatedSwipeable')
        return { __esModule: true, default: Swipeable };
      if (name === 'react-native-reanimated')
        return {
          __esModule: true,
          default: { View: Capture },
          ReduceMotion: { Always: 'always', System: 'system' },
          useSharedValue: (v: number) => {
            const ref = React.useRef(v);
            return React.useMemo(
              () => ({
                get: () => ref.current,
                set: (next: number) => {
                  ref.current = next;
                },
              }),
              [],
            );
          },
          useAnimatedStyle: (fn: () => unknown) => fn(),
        };
      if (name === '@expo/ui/community/menu') return { MenuView: Capture };
      if (name === 'fixture:client')
        return {
          useClient: () => ({
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
            store,
          }),
        };
      if (name === 'fixture:theme')
        return {
          useTheme: () => ({
            presentation: telegramPresentation('day'),
            reducedMotion: false,
            colors: { accent: telegramPresentation('day').accent },
          }),
        };
      if (name === 'fixture:i18n')
        return { ...localization, formatLocale: locale === 'en' ? 'en-US' : 'pt-BR' };
      if (name === 'fixture:home')
        return {
          useHome: () => ({
            items: [],
            editing: false,
            ownerKey: 'owner',
            isPinned: () => false,
            hiddenItems: [],
          }),
        };
      if (name === 'fixture:primitives')
        return {
          EmptyState: () => null,
          Icon: () => null,
          NativeButton: Capture,
          IconButton: Capture,
        };
      if (name === 'fixture:avatar') return { ProfileAvatar: () => null };
      if (name === 'fixture:receipt') return { MessageReceipt: () => null };
      if (name === 'fixture:chatsActions') return { ChatsActions: () => null };
      if (name === 'fixture:rail') return { MiniappRail: () => null };
      if (name === 'fixture:cardControls') return { AppCardEditControls: () => null };
      throw Error(name);
    },
  });
  await root.render(React.createElement(module.exports.default));
  return {
    core,
    store,
    calls,
    routes,
    haptics,
    projections,
    errors: root.errors,
    act: root.act,
    t: localization.t,
    has: (id: string) => controls.has(id),
    control: (id: string) => {
      const value = controls.get(id);
      assert.ok(value, id);
      return value;
    },
    reveal: async (id: string, edge: 'leading' | 'trailing') =>
      root.act(() => {
        const value = swipes.get(`inbox-swipe-${id}`);
        assert.ok(value);
        value.reveal(edge);
      }),
    layout: (fn: typeof layout) => {
      layout = fn;
    },
    snapshot: (fn: typeof snapshot) => {
      snapshot = fn;
    },
    focus: (value: boolean) => {
      focused = value;
      focusListeners.forEach((fn) => fn(value));
    },
    app: (value: string) => {
      foreground = value;
      appListeners.forEach((fn) => fn(value));
    },
    reopen: async () => {
      await store.close();
      await store.open('account');
    },
    close: async () => {
      await root.close();
      await store.close();
    },
  };
}
