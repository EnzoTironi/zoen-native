import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as flow from '../notifications/muted-chats.ts';
import * as notificationSettings from '../notifications/settings-presentation.ts';
import * as ownership from './account-scope.ts';
import { ClientStore } from './store.ts';
import { Driver } from './store.fixtures.ts';
import { mutedSnapshot } from './muted-chats.fixtures.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { notifications } from '../i18n/catalogs/notifications.ts';
import { common } from '../i18n/catalogs/common.ts';
import { settings } from '../i18n/catalogs/settings.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { ownerPolicy } from '../i18n/catalogs/owner-policy.ts';
import { telegramPresentation } from '../theme/telegram.ts';
export type Element = { type: any; props: Record<string, any> };
const files = {
  hook: '../notifications/use-muted-chats.ts',
  view: '../notifications/muted-chats-screen.tsx',
  route: '../app/settings/muted-chats.tsx',
  entry: '../components/notification-settings.tsx',
  settingsScreen: '../components/settings-screen.tsx',
  you: '../app/(tabs)/you.tsx',
};
const compiled = new Map(
  await Promise.all(
    Object.entries(files).map(async ([name, path]) => {
      const result = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-ports',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [name, result.outputFiles[0]!.text] as const;
    }),
  ),
);
export function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
export function strings(value: unknown): string {
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(strings).join(' ');
  return value && typeof value === 'object' && 'props' in value
    ? strings((value as Element).props.children)
    : '';
}
export async function settle() {
  for (let n = 0; n < 30; ++n) await Promise.resolve();
}
export async function mutedUi(locale: 'en' | 'pt-BR' = 'en') {
  const localization = createLocalization({ notifications, common, settings, chat, ownerPolicy }, [
    locale,
  ]);
  const snapshot = mutedSnapshot(),
    driver = new Driver();
  driver.opening = async () => structuredClone(snapshot);
  const calls: any[] = [],
    routes: any[] = [],
    dialogs: any[] = [];
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'set_chat_muted') {
      snapshot.conversations = snapshot.conversations.map((row) =>
        row.id === command.space_id ? { ...row, muted: false, muted_until_ms: null } : row,
      );
      return;
    }
    throw new Error('Unexpected Core command');
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const foreground = new Set<(value: string) => void>();
  const appState = {
    currentState: 'active',
    addEventListener: (_: string, fn: (value: string) => void) => {
      foreground.add(fn);
      return {
        remove: () => {
          foreground.delete(fn);
        },
      };
    },
  };
  let confirm: (options: any) => Promise<boolean> = async () => false;
  let cursor = 0;
  const slots: any[] = [];
  const stateSlots: any[] = [];
  const memo = (factory: () => any, deps: unknown[]) => {
    const index = cursor++;
    if (!slots[index] || !deps.every((value, i) => Object.is(value, slots[index].deps[i])))
      slots[index] = { value: factory(), deps };
    return slots[index].value;
  };
  let focused: (() => void | (() => void)) | undefined, nextFocus: typeof focused;
  let cleanup: (() => void) | undefined;
  const loaded = new Map<string, any>();
  const require = (name: string): any => {
    if (name === 'react')
      return {
        useState: (initial: unknown) => {
          const index = cursor++;
          if (!(index in stateSlots)) stateSlots[index] = initial;
          return [
            stateSlots[index],
            (value: unknown) => {
              stateSlots[index] = value;
            },
          ];
        },
        useMemo: memo,
        useCallback: (fn: () => any, deps: unknown[]) => memo(() => fn, deps),
        useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
      };
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: unknown, props: unknown) => ({ type, props }),
        jsxs: (type: unknown, props: unknown) => ({ type, props }),
        Fragment: 'Fragment',
      };
    if (name === 'react-native')
      return {
        AppState: appState,
        Platform: { OS: 'ios' },
        Linking: {},
        FlatList: 'FlatList',
        ScrollView: 'ScrollView',
        View: 'View',
        Text: 'Text',
        ActivityIndicator: 'ActivityIndicator',
      };
    if (name === 'expo-router')
      return {
        Stack: { Screen: 'Stack.Screen' },
        router: { push: (route: unknown) => routes.push(route) },
        useFocusEffect: (fn: typeof focused) => {
          if (fn !== focused) nextFocus = fn;
        },
      };
    if (name === '@expo/ui')
      return {
        BottomSheet: 'BottomSheet',
        RNHostView: 'RNHostView',
        Host: 'Host',
        Column: 'Column',
        Text: 'NativeText',
        Collapsible: 'Collapsible',
      };
    if (name === './settings-screen') return load('settingsScreen');
    if (name === './navigation-scroll-view') return { NavigationScrollView: 'ScrollView' };
    if (name.endsWith('/notifications/settings-presentation')) return notificationSettings;
    if (name.endsWith('/core/provider'))
      return { useClient: () => ({ ...store.getSnapshot(), store }) };
    if (name.endsWith('/account-scope')) return ownership;
    if (name === './muted-chats') return flow;
    if (name.endsWith('/use-muted-chats')) return load('hook');
    if (name.endsWith('/muted-chats-screen')) return load('view');
    if (name.endsWith('/i18n') || name.endsWith('/i18n/index.ts')) return localization;
    if (name.endsWith('/theme'))
      return {
        useTheme: () => ({
          presentation: telegramPresentation('day'),
          colors: { text: '#111111', accent: '#007AFF' },
        }),
      };
    if (name.endsWith('/components/dialogs'))
      return {
        useDialogs: () => ({
          confirm: (options: unknown) => {
            dialogs.push(options);
            return confirm(options);
          },
        }),
      };
    if (name.endsWith('/components/primitives') || name === './primitives')
      return {
        NativeButton: 'NativeButton',
        IconButton: 'IconButton',
        EmptyState: 'EmptyState',
        Icon: 'Icon',
        Surface: 'Surface',
      };
    if (name.endsWith('/components/settings-rows') || name === './settings-rows')
      return { SettingsRows: 'SettingsRows' };
    if (name.endsWith('/notifications/provider'))
      return {
        useNotifications: () => ({
          permission: 'allowed',
          registration: 'acknowledged',
          busy: false,
          failure: null,
          deliveryVerified: false,
        }),
      };
    if (name.endsWith('/profile/use-profile-photo')) return { useProfilePhoto: () => undefined };
    if (name.endsWith('/components/sky-background')) return { SkyScene: 'SkyScene' };
    if (name.endsWith('/profile/profile-header')) return { ProfileHeader: 'ProfileHeader' };
    if (name.endsWith('/profile/profile-section'))
      return { ProfileSection: 'ProfileSection', ProfileInfoRow: 'ProfileInfoRow' };
    if (name.endsWith('/integrity/integrity-warning'))
      return { IntegrityWarnings: 'IntegrityWarnings' };
    throw new Error(`Unexpected platform port ${name}`);
  };
  function load(name: string): any {
    if (loaded.has(name)) return loaded.get(name);
    const module = { exports: {} };
    runInNewContext(compiled.get(name)!, { module, require, process: { env: { EXPO_OS: 'ios' } } });
    loaded.set(name, module.exports);
    return module.exports;
  }
  function render(name = 'route') {
    cursor = 0;
    const tree = load(name).default();
    if (nextFocus) {
      cleanup?.();
      focused = nextFocus;
      nextFocus = undefined;
      cleanup = focused!() || undefined;
    }
    return tree;
  }
  const route = () => {
    const tree = render();
    const view = nodes(tree).find((node) => node.type === load('view').MutedChatsScreen)!;
    return { props: view.props, tree: view.type(view.props) as Element };
  };
  return {
    store,
    driver,
    snapshot,
    calls,
    routes,
    dialogs,
    route,
    t: localization.t,
    entry: () => {
      cursor = 0;
      const screen = load('entry').NotificationSettings();
      return screen.type(screen.props);
    },
    you: () => {
      const tree = render('you');
      return tree.props.children.type(tree.props.children.props);
    },
    confirm: (fn: typeof confirm) => {
      confirm = fn;
    },
    foreground: (value: string) => {
      appState.currentState = value;
      for (const fn of foreground) fn(value);
    },
    blur: () => {
      cleanup?.();
      cleanup = undefined;
      focused = undefined;
    },
    close: async () => {
      cleanup?.();
      await store.close();
    },
  };
}
