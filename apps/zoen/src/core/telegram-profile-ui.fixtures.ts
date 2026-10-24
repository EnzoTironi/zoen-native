import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { sharedItems, sharedLinks, sharedMediaTab } from '../profile/shared-media-projection.ts';
import { peerInfo, profileTitleCollapseOffset } from '../profile/telegram-profile-presentation.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import type { ItemDetail, TimelineEntry } from './types.ts';

export type Element = { type: unknown; props: Record<string, any> };
export type Handler = (...args: any[]) => any;
export const sources = {
  edit: '../app/edit-profile.tsx',
  header: '../profile/profile-header.tsx',
  tile: '../profile/shared-media-tile.tsx',
  person: '../app/person/[id].tsx',
  space: '../app/space/[id].tsx',
  you: '../app/(tabs)/you.tsx',
  shared: '../app/shared-media/[id].tsx',
} as const;
export const compiled = new Map(
  await Promise.all(
    Object.entries(sources).map(async ([key, path]) => {
      const output = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        platform: 'node',
        format: 'cjs',
        bundle: true,
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'actual-screen-platform-ports',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [key, output.outputFiles[0]!.text] as const;
    }),
  ),
);
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((ok, no) => {
    resolve = ok;
    reject = no;
  });
  return { promise, resolve, reject };
}
export async function settle() {
  for (let step = 0; step < 12; step++) await Promise.resolve();
}
export function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
export function target(tree: Element, id: string) {
  const node = nodes(tree).find((node) => node.props.testID === id);
  assert.ok(node, id);
  return node;
}
export const name = (id: string) => ({
  id,
  kind: 'Person',
  name: id,
  handle: id,
  is_me: id === 'me',
  tint_hex: '#336699',
  photo_sha256: null,
  photo_ready: false,
});
export function item(id: string, mime = 'image/jpeg', extra: Partial<ItemDetail> = {}): ItemDetail {
  return {
    id,
    space_id: 'space',
    space_title: 'Shared',
    kind_id: 'file',
    kind_label: 'File',
    title: id,
    origin: '',
    created_by: name('me'),
    plan: null,
    text: null,
    version: 4,
    versions: [],
    linked_requests: [],
    app: null,
    path: '',
    file: {
      name: id,
      mime,
      bytes: '5',
      sha256: 'aa'.repeat(32),
      ready: true,
      downloaded_bytes: '5',
      download_error: null,
    },
    ...extra,
  } as ItemDetail;
}
export function message(
  id: string,
  text: string,
  extra: Partial<TimelineEntry> = {},
): TimelineEntry {
  return {
    id,
    seq: '1',
    author: name('me'),
    at_ms: '1',
    kind: { Message: { text, mentions: [] } },
    delivery: 'Read',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
    ...extra,
  } as TimelineEntry;
}
export function fixture(kind: keyof typeof sources, overrides: Record<string, unknown> = {}) {
  type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void; set?: Handler };
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  let dirty = false;
  let focusCleanup: (() => void) | undefined;
  const equal = (a?: readonly unknown[], b?: readonly unknown[]) =>
    Boolean(
      a && b && a.length === b.length && a.every((value, index) => Object.is(value, b[index])),
    );
  const slot = () => (slots[cursor++] ??= {});
  const effect = (run: () => void | (() => void), deps?: readonly unknown[]) => {
    const current = slot();
    if (equal(current.deps, deps)) return;
    current.deps = deps;
    effects.push(() => {
      current.cleanup?.();
      current.cleanup = run() ?? undefined;
    });
  };
  const memo = (run: Handler, deps: readonly unknown[]) => {
    const current = slot();
    if (!equal(current.deps, deps)) {
      current.deps = deps;
      current.value = run();
    }
    return current.value;
  };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const routes: unknown[] = [];
  const requests: any[] = [];
  const releases: string[] = [];
  const saves: any[] = [];
  const previews: unknown[] = [];
  const state = {
    mode: 'account',
    snapshot: {
      account: { identity_id: 'me', device_id: 'device', handle: 'me_user' },
      me: { ...name('me'), bio: 'About', name: 'Me' },
      conversations: [
        {
          id: 'space',
          title: 'Shared',
          kind: 'Group',
          members: [name('me'), name('friend')],
          muted: false,
          pinned: false,
          archived: false,
          privacy: 'EndToEnd',
          event_count: 4,
        },
      ],
      agents: [],
    },
    timelines: {} as Record<string, any>,
  };
  const store = {
    sessionEpoch: 1,
    driver: {
      photos: {
        save: async (request: unknown) => {
          saves.push(request);
        },
      },
      media: {
        materializeFile: async (id: string, version: number) => ({
          uri: `local:${id}`,
          itemId: id,
          version,
          info: { name: id, mime: 'image/jpeg', bytes: '5', sha256: 'aa'.repeat(32) },
        }),
        releaseLocalFile: async (uri: string) => {
          releases.push(uri);
        },
      },
      request: async (command: unknown) => {
        requests.push(command);
        return { link: 'invite' };
      },
    },
    getSnapshot: () => state,
    refresh: async () => {},
    request: async (command: unknown) => {
      requests.push(command);
      return 'new-space';
    },
    loadTimeline: async () => {},
  };
  let picker: Handler = async () => null;
  let query: any = {
    data: { ...name('friend'), bio: 'Friend bio', blocked: false },
    error: null,
    retry() {},
  };
  const router = {
    push: (route: unknown) => {
      routes.push(['push', route]);
    },
    back: () => {
      routes.push(['back']);
    },
    dismissTo: (route: unknown) => {
      routes.push(['dismissTo', route]);
    },
  };
  const hooks = {
    useState(initial: unknown) {
      const current = slot();
      if (!('value' in current))
        current.value = typeof initial === 'function' ? initial() : initial;
      current.set ??= (next: unknown) => {
        const value = typeof next === 'function' ? next(current.value) : next;
        if (!Object.is(current.value, value)) {
          current.value = value;
          dirty = true;
        }
      };
      return [current.value, current.set];
    },
    useRef(initial: unknown) {
      const current = slot();
      current.value ??= { current: initial };
      return current.value;
    },
    useEffect: effect,
    useLayoutEffect: effect,
    useCallback: (fn: Handler, deps: readonly unknown[]) => memo(() => fn, deps),
    useMemo: memo,
  };
  const p = telegramPresentation('night');
  const primitives = Object.fromEntries(
    ['Avatar', 'Field', 'Icon', 'EmptyState', 'Loading', 'NativeButton', 'Surface'].map((key) => [
      key,
      key,
    ]),
  );
  const platform = {
    Pressable: 'Pressable',
    ScrollView: 'ScrollView',
    View: 'View',
    Text: 'Text',
    ActivityIndicator: 'ActivityIndicator',
    Share: {
      share: async (data: unknown) => {
        routes.push(['share', data]);
      },
    },
    Linking: {
      openURL: async (url: string) => {
        routes.push(['external', url]);
      },
    },
  };
  const ports: Record<string, any> = {
    react: hooks,
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': platform,
    'expo-router': {
      router,
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'friend' }),
      useFocusEffect: (fn: Handler) =>
        effect(() => {
          const cleanup = fn();
          focusCleanup = cleanup;
          return cleanup;
        }, [fn]),
    },
    'expo-image': { Image: 'Image' },
    '@shopify/flash-list': { FlashList: 'FlashList' },
    '@expo/ui': { Host: 'Host', Column: 'Column', Switch: 'Switch' },
  };
  const t = (key: string, params?: unknown) => (params ? `${key}:${JSON.stringify(params)}` : key);
  function requirePort(path: string): any {
    if (path in overrides)
      return path === 'expo-router'
        ? { ...ports[path], ...(overrides[path] as Record<string, unknown>) }
        : overrides[path];
    if (path in ports) return ports[path];
    if (/\/i18n(?:\/index.ts)?$/.test(path)) return { t, formatLocale: () => 'en-US' };
    if (path.endsWith('core/provider')) return { useClient: () => ({ ...state, store }) };
    if (path.endsWith('core/query')) return { useCoreQuery: () => query };
    // This fixture tests the existing profile surface; member controls have their own real-port suite.
    if (path.endsWith('communities/use-member-actions'))
      return { useMemberActions: () => ({ available: false, session: {}, state: {} }) };
    if (path.endsWith('communities/member-actions-sheet'))
      return { MemberActionsSheet: 'MemberActionsSheet' };
    if (path.endsWith('components/primitives')) return primitives;
    if (path.endsWith('components/dialogs'))
      return { useDialogs: () => ({ confirm: async () => true }) };
    if (path.endsWith('profile/picker')) return { pickProfilePhoto: () => picker() };
    if (path.endsWith('profile/use-profile-photo'))
      return { useProfilePhoto: () => 'local:avatar' };
    if (path.endsWith('profile/profile-section'))
      return { ProfileSection: 'ProfileSection', ProfileInfoRow: 'ProfileInfoRow' };
    if (path.endsWith('profile/profile-header')) return { ProfileHeader: 'ProfileHeader' };
    if (path.endsWith('profile/profile-avatar')) return { ProfileAvatar: 'ProfileAvatar' };
    if (path.endsWith('telegram-profile-presentation'))
      return { peerInfo, profileTitleCollapseOffset };
    if (path.endsWith('theme'))
      return {
        useTheme: () => ({
          presentation: p,
          scheme: 'dark',
          colors: { text: p.text, secondary: p.secondary, danger: p.destructive },
          preferenceError: null,
        }),
      };
    if (path === 'react-native-safe-area-context')
      return { useSafeAreaInsets: () => ({ top: 62, bottom: 34, left: 0, right: 0 }) };
    if (path.endsWith('media/preview-file'))
      return {
        previewMediaFile: async (_driver: unknown, file: any) => {
          previews.push(file);
          await store.driver.media.releaseLocalFile(file.uri);
        },
      };
    if (path.endsWith('media/file-size'))
      return { formatFileSize: (bytes: unknown) => `${bytes} B` };
    if (path.endsWith('profile/shared-media-projection'))
      return {
        sharedItems,
        sharedLinks,
        sharedMediaTab,
        sharedMediaTabs: ['media', 'files', 'audio', 'links', 'apps'],
      };
    if (path.endsWith('profile/shared-media-tile')) return { SharedMediaTile: 'SharedMediaTile' };
    if (path.endsWith('integrity/integrity-warning'))
      return { IntegrityWarning: 'IntegrityWarning', IntegrityWarnings: 'IntegrityWarnings' };
    if (path.endsWith('components/settings-rows')) return { SettingsRows: 'SettingsRows' };
    if (path.endsWith('components/sky-background')) return { SkyScene: 'SkyScene' };
    throw new Error(`Missing platform port: ${path}`);
  }
  const module = { exports: {} as Record<string, any> };
  runInNewContext(compiled.get(kind)!, {
    module,
    exports: module.exports,
    require: requirePort,
    URL,
    Error,
    console,
    setTimeout,
    clearTimeout,
  });
  function render(props: Record<string, unknown> = {}, component?: Handler): Element {
    let tree!: Element;
    let pass = 0;
    do {
      assert.ok(++pass < 8);
      cursor = 0;
      dirty = false;
      effects = [];
      tree = (
        component ??
        module.exports.default ??
        module.exports.ProfileHeader ??
        module.exports.SharedMediaTile
      )(props);
      for (const run of effects) run();
    } while (dirty);
    return tree;
  }
  function unmount() {
    for (const current of slots) {
      current.cleanup?.();
      current.cleanup = undefined;
    }
  }
  return {
    render,
    unmount,
    blur: () => {
      focusCleanup?.();
    },
    routes,
    requests,
    releases,
    saves,
    previews,
    state,
    store,
    exports: module.exports,
    setPicker: (next: Handler) => {
      picker = next;
    },
    setQuery: (next: unknown) => {
      query = next;
    },
  };
}
