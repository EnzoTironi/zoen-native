import { memberRoleLabelKey } from '../communities/member-role-label.ts';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as flow from '../communities/member-actions.ts';
import * as addition from '../communities/member-addition.ts';
import * as port from '../communities/member-context-port.ts';
import * as ownership from './account-scope.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { context, peer, settle } from './community-members.fixtures.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { community } from '../i18n/catalogs/community.ts';
import { conversationCards } from '../i18n/catalogs/conversation-cards.ts';
import { common } from '../i18n/catalogs/common.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { peerInfo as peerCatalog } from '../i18n/catalogs/peer-info.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { peerInfo, profileTitleCollapseOffset } from '../profile/telegram-profile-presentation.ts';
export { settle };
export type Element = { type: any; props: Record<string, any> };
const compiled = new Map(
  await Promise.all(
    Object.entries({
      hook: '../communities/use-member-actions.ts',
      sheet: '../communities/member-actions-sheet.tsx',
      route: '../app/space/[id].tsx',
    }).map(async ([name, path]) => {
      const result = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        platform: 'node',
        format: 'cjs',
        bundle: true,
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'actual-community-ui-native-ports',
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
export async function communityUi(
  locale: 'en' | 'pt-BR' = 'en',
  platform: 'ios' | 'android' | 'web' = 'ios',
) {
  const localization = createLocalization(
    { community, common, chat, conversationCards, peerInfo: peerCatalog },
    [locale],
  );
  const snapshot = draftSession();
  const room = snapshot.conversations[0]!;
  room.kind = 'Community';
  room.title = 'Selected community';
  room.members.push(context().member!);
  let current = {
    ...context(),
    space_id: room.id,
    space_title: room.title,
    identity_id: snapshot.account!.identity_id,
    device_id: snapshot.account!.device_id,
  };
  const calls: any[] = [],
    dialogs: any[] = [],
    routes: any[] = [];
  const driver = new Driver();
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'space_member_context')
      return structuredClone(
        command.identity_id === snapshot.account!.identity_id
          ? { ...current, member_id: command.identity_id }
          : current,
      );
    if (command.op === 'remove_member_reviewed') {
      current = { ...current, state: 'Removing', can_remove: false };
      room.members = room.members.filter((member) => member.id !== peer);
      return structuredClone(current);
    }
    if (command.op === 'set_member_role_reviewed') {
      current = {
        ...current,
        state: 'Updating',
        can_remove: false,
        role_options: [],
        pending_role: command.role,
      };
      return structuredClone(current);
    }
    throw Error('Unexpected community command ' + command.op);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  let answer: (options: any) => Promise<boolean> = async () => false;
  let cursor = 0;
  const slots: any[] = [],
    effects: Array<() => void> = [];
  const next = () => (slots[cursor++] ??= {});
  const same = (a: unknown[] | undefined, b: unknown[] | undefined) =>
    Boolean(a && b && a.length === b.length && a.every((value, n) => Object.is(value, b[n])));
  const memo = (factory: () => any, deps: unknown[]) => {
    const item = next();
    if (!same(item.deps, deps)) {
      item.value = factory();
      item.deps = deps;
    }
    return item.value;
  };
  const focusCleanups = new Set<() => void>();
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
  const react = {
    useMemo: memo,
    useCallback: (fn: () => any, deps: unknown[]) => memo(() => fn, deps),
    useRef: (value: any) => {
      const item = next();
      return (item.value ??= { current: value });
    },
    useState: (initial: any) => {
      const item = next();
      if (!('value' in item)) item.value = typeof initial === 'function' ? initial() : initial;
      return [
        item.value,
        (value: any) => {
          item.value = typeof value === 'function' ? value(item.value) : value;
        },
      ];
    },
    useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
  };
  const loaded = new Map<string, any>();
  const require = (name: string): any => {
    if (name === 'react') return react;
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: any, props: any) => ({ type, props }),
        jsxs: (type: any, props: any) => ({ type, props }),
        Fragment: 'Fragment',
      };
    if (name === 'react-native')
      return {
        AppState: appState,
        Platform: { OS: platform },
        View: 'View',
        Text: 'Text',
        Pressable: 'Pressable',
        ScrollView: 'ScrollView',
        Share: { share: async () => {} },
      };
    if (name === '@expo/ui')
      return {
        Host: 'Host',
        Column: 'Column',
        Switch: 'Switch',
        BottomSheet: 'BottomSheet',
        RNHostView: 'RNHostView',
      };
    if (name === './member-role-label') return { memberRoleLabelKey };
    if (name === '@expo/ui/community/menu') return { MenuView: 'MenuView' };
    if (name === 'react-native-safe-area-context')
      return { useSafeAreaInsets: () => ({ top: 50, bottom: 20, left: 0, right: 0 }) };
    if (name === 'expo-router')
      return {
        Stack: { Screen: 'Stack.Screen' },
        useLocalSearchParams: () => ({ id: room.id }),
        router: {
          push: (value: unknown) => routes.push(value),
          dismissTo: (value: unknown) => routes.push(value),
        },
        useFocusEffect: (fn: () => void | (() => void)) => {
          const item = next();
          if (item.fn !== fn) {
            item.fn = fn;
            effects.push(() => {
              item.cleanup?.();
              const cleanup = fn();
              item.cleanup = cleanup;
              if (cleanup) focusCleanups.add(cleanup);
            });
          }
        },
      };
    if (name.endsWith('/core/provider'))
      return { useClient: () => ({ ...store.getSnapshot(), store }) };
    if (name.endsWith('/account-scope')) return ownership;
    if (name === './member-actions') return flow;
    if (name === './member-addition') return addition;
    if (name === './member-context-port') return port;
    if (name.endsWith('/use-member-actions')) return load('hook');
    if (name.endsWith('/member-addition-sheet'))
      return { MemberAdditionSheet: 'MemberAdditionSheet' };
    if (name.endsWith('/member-actions-sheet')) return load('sheet');
    if (name.endsWith('/i18n') || name.endsWith('/i18n/index.ts')) return localization;
    if (name.endsWith('/theme'))
      return { useTheme: () => ({ presentation: telegramPresentation('day'), scheme: 'light' }) };
    if (name.endsWith('/components/dialogs'))
      return {
        useDialogs: () => ({
          confirm: (options: unknown) => {
            dialogs.push(options);
            return answer(options);
          },
        }),
      };
    if (name.endsWith('/components/primitives'))
      return {
        Icon: 'Icon',
        EmptyState: 'EmptyState',
        Loading: 'Loading',
        IconButton: 'IconButton',
        NativeButton: 'NativeButton',
      };
    if (name.endsWith('/components/settings-rows')) return { SettingsRows: 'SettingsRows' };
    if (name.endsWith('/profile/profile-avatar')) return { ProfileAvatar: 'ProfileAvatar' };
    if (name.endsWith('/profile/profile-header')) return { ProfileHeader: 'ProfileHeader' };
    if (name.endsWith('/profile/profile-section'))
      return { ProfileSection: 'ProfileSection', ProfileInfoRow: 'ProfileInfoRow' };
    if (name.endsWith('/profile/use-profile-photo')) return { useProfilePhoto: () => undefined };
    if (name.endsWith('/profile/telegram-profile-presentation'))
      return { peerInfo, profileTitleCollapseOffset };
    if (name.endsWith('/integrity/integrity-warning'))
      return { IntegrityWarning: 'IntegrityWarning' };
    throw Error('Unexpected platform port ' + name);
  };
  function load(name: string): any {
    if (loaded.has(name)) return loaded.get(name);
    const module = { exports: {} };
    runInNewContext(compiled.get(name)!, { module, require, process: { env: { EXPO_OS: 'ios' } } });
    loaded.set(name, module.exports);
    return module.exports;
  }
  function render() {
    cursor = 0;
    const tree = load('route').default();
    for (const effect of effects.splice(0)) effect();
    const sheet = nodes(tree).find((node) => node.type === load('sheet').MemberActionsSheet);
    return { tree, props: sheet?.props, sheet: sheet ? sheet.type(sheet.props) : null };
  }
  return {
    render,
    store,
    driver,
    snapshot,
    calls,
    dialogs,
    routes,
    room,
    t: localization.t,
    context: () => current,
    value: (value: typeof current) => {
      current = value;
    },
    confirm: (fn: typeof answer) => {
      answer = fn;
    },
    foreground: (value: string) => {
      appState.currentState = value;
      for (const fn of foreground) fn(value);
    },
    blur: () => {
      for (const stop of focusCleanups) stop();
      focusCleanups.clear();
    },
    close: async () => {
      for (const stop of focusCleanups) stop();
      await store.close();
    },
  };
}
