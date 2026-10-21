import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';
import { peerInfo, profileTitleCollapseOffset } from '../profile/telegram-profile-presentation.ts';
import { sharedItems, sharedLinks, sharedMediaTab } from '../profile/shared-media-projection.ts';
import type { ItemDetail, TimelineEntry } from './types.ts';

type Element = { type: unknown; props: Record<string, any> };
type Handler = (...args: any[]) => any;
const sources = {
  edit: '../app/edit-profile.tsx',
  header: '../profile/profile-header.tsx',
  tile: '../profile/shared-media-tile.tsx',
  person: '../app/person/[id].tsx',
  space: '../app/space/[id].tsx',
  you: '../app/(tabs)/you.tsx',
  shared: '../app/shared-media/[id].tsx',
} as const;
const compiled = new Map(
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
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((ok, no) => {
    resolve = ok;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function settle() {
  for (let step = 0; step < 12; step++) await Promise.resolve();
}
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
function target(tree: Element, id: string) {
  const node = nodes(tree).find((node) => node.props.testID === id);
  assert.ok(node, id);
  return node;
}
const name = (id: string) => ({
  id,
  kind: 'Person',
  name: id,
  handle: id,
  is_me: id === 'me',
  tint_hex: '#336699',
  photo_sha256: null,
  photo_ready: false,
});
function item(id: string, mime = 'image/jpeg', extra: Partial<ItemDetail> = {}): ItemDetail {
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
function message(id: string, text: string, extra: Partial<TimelineEntry> = {}): TimelineEntry {
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
function fixture(kind: keyof typeof sources, overrides: Record<string, unknown> = {}) {
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

test('profile header uses the pinned100pt avatar,28pt title and58pt real action tiles; self has no fake calls', () => {
  const f = fixture('header');
  let tree = f.render({ name: 'Ana', subtitle: '@ana', actions: [] });
  assert.equal(nodes(tree).find((node) => node.type === 'Avatar')!.props.size, 100);
  assert.equal(target(tree, 'peer-info-title').props.style.fontSize, 28);
  assert.equal(
    nodes(tree).some((node) => node.props.testID === 'peer-info-actions'),
    false,
  );
  let called = 0;
  tree = f.render({
    name: 'Ana',
    actions: [
      {
        id: 'message',
        label: 'Message',
        icon: 'bubble',
        onPress: () => {
          called++;
        },
      },
    ],
  });
  const action = target(tree, 'message');
  assert.equal(action.props.style({ pressed: false }).height, 58);
  action.props.onPress();
  assert.equal(called, 1);
});
test('You retains Appearance, all settings and the actual Edit Profile entry after header port', () => {
  const f = fixture('you');
  const outer = f.render();
  // You wraps its content in SkyScene. Invoke that actual component, with its hooks isolated here.
  const content = nodes(outer).find((node) => typeof node.type === 'function');
  assert.ok(content);
  const tree = (content.type as Handler)(content.props);
  const settings = target(tree, 'you-settings-rows').props.rows;
  assert.deepEqual(
    Array.from(settings, (row: any) => row.id),
    [
      'settings-appearance',
      'settings-general',
      'settings-permissions',
      'settings-notifications',
      'settings-devices',
      'settings-data-controls',
    ],
  );
  settings.find((row: any) => row.id === 'settings-appearance').onPress();
  assert.deepEqual(f.routes[0], ['push', '/settings/appearance']);
  const header = nodes(tree).find((node) => node.type === 'ProfileHeader')!;
  header.props.onAvatarPress();
  assert.deepEqual(f.routes[1], ['push', '/edit-profile']);
});
test('Edit Profile blocks invalid handle and coalesces synchronous Save taps', async () => {
  const f = fixture('edit');
  let tree = f.render();
  nodes(tree)
    .find((node) => node.type === 'Field' && node.props.label === 'profile.handle')!
    .props.onChangeText('bad handle');
  tree = f.render();
  let save = nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight();
  assert.equal(save.props.disabled, true);
  save.props.onPress();
  await settle();
  assert.equal(f.saves.length, 0);
  nodes(tree)
    .find((node) => node.type === 'Field' && node.props.label === 'profile.handle')!
    .props.onChangeText('new_handle');
  tree = f.render();
  save = nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight();
  save.props.onPress();
  save.props.onPress();
  await settle();
  assert.equal(f.saves.length, 1);
  assert.equal(f.requests.filter((request) => request.op === 'update_handle').length, 1);
  assert.deepEqual(f.routes, [['back']]);
});
test('pending photo picker after departure/account change releases its draft without publishing', async () => {
  for (const depart of ['unmount', 'epoch'] as const) {
    const f = fixture('edit');
    const picked = deferred<any>();
    let disposed = 0;
    f.setPicker(() => picked.promise);
    const tree = f.render();
    target(tree, 'edit-profile-photo').props.onPress();
    if (depart === 'unmount') f.unmount();
    else f.store.sessionEpoch++;
    picked.resolve({
      uri: 'local:pick',
      dispose() {
        disposed++;
      },
    });
    await settle();
    assert.equal(disposed, 1);
    assert.equal(f.saves.length, 0);
    assert.equal(f.routes.length, 0);
  }
});
test('pending native save retains exact photo URI through unmount and never navigates late', async () => {
  const f = fixture('edit');
  let disposed = 0;
  f.setPicker(async () => ({
    uri: 'local:owned',
    dispose() {
      disposed++;
    },
  }));
  target(f.render(), 'edit-profile-photo').props.onPress();
  await settle();
  const pending = deferred<void>();
  f.store.driver.photos.save = async (request) => {
    f.saves.push(request);
    await pending.promise;
  };
  const tree = f.render();
  const save = nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight();
  save.props.onPress();
  await settle();
  assert.equal(f.saves[0].uri, 'local:owned');
  f.unmount();
  assert.equal(disposed, 0);
  pending.resolve();
  await settle();
  assert.equal(disposed, 1);
  assert.equal(f.routes.length, 0);
});
test('failed native photo save preserves displayed draft for retry and explicit Remove disposes once', async () => {
  const f = fixture('edit');
  let disposed = 0;
  f.setPicker(async () => ({
    uri: 'local:owned',
    dispose() {
      disposed++;
    },
  }));
  target(f.render(), 'edit-profile-photo').props.onPress();
  await settle();
  f.store.driver.photos.save = async () => {
    throw new Error('denied');
  };
  const tree = f.render();
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  const failed = f.render();
  assert.equal(disposed, 0);
  assert.ok(
    nodes(failed).some(
      (node) => node.props.accessibilityRole === 'alert' && node.props.children === 'denied',
    ),
  );
  target(failed, 'edit-profile-remove-photo').props.onPress();
  f.unmount();
  assert.equal(disposed, 1);
});
test('session epoch/device change rejects stale Save before mutation, including between handle and photo', async () => {
  const f = fixture('edit');
  let tree = f.render();
  f.store.sessionEpoch++;
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  assert.equal(f.saves.length, 0);
  assert.equal(f.requests.length, 0);
  const next = fixture('edit');
  tree = next.render();
  nodes(tree)
    .find((node) => node.type === 'Field' && node.props.label === 'profile.handle')!
    .props.onChangeText('new_handle');
  next.store.request = async (command) => {
    next.requests.push(command);
    next.state.snapshot.account = { ...next.state.snapshot.account, device_id: 'retired' };
    return '';
  };
  tree = next.render();
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  assert.equal(next.requests.length, 1);
  assert.equal(next.saves.length, 0);
  assert.equal(next.routes.length, 0);
});
test('Person real start-direct is coalesced and late success cannot navigate a retired owner', async () => {
  const f = fixture('person');
  const pending = deferred<string>();
  f.store.request = async (command) => {
    f.requests.push(command);
    return pending.promise;
  };
  const tree = f.render();
  const action = nodes(tree).find((node) => node.type === 'ProfileHeader')!.props.actions[0];
  action.onPress();
  action.onPress();
  assert.equal(f.requests.length, 1);
  f.unmount();
  pending.resolve('space');
  await settle();
  assert.equal(f.routes.length, 0);
});
test('conversation details preferences use fresh canonical unrelated flags; shared-media targets same chat', async () => {
  const f = fixture('space', {
    'expo-router': {
      router: {
        push: (route: unknown) => {
          f.routes.push(route);
        },
      },
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'space' }),
    },
  });
  const tree = f.render();
  f.state.snapshot.conversations[0]!.archived = true;
  const mute = nodes(tree)
    .find((node) => node.type === 'ProfileHeader')!
    .props.actions.find((action: any) => action.id === 'details-mute');
  mute.onPress();
  await settle();
  assert.deepEqual(JSON.parse(JSON.stringify(f.requests[0])), {
    op: 'set_chat_prefs',
    space_id: 'space',
    pinned: false,
    muted: true,
    archived: true,
  });
  target(tree, 'details-shared-media').props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(f.routes[0])), {
    pathname: '/shared-media/[id]',
    params: { id: 'space' },
  });
});
test('shared media projection uses exact authorized space/current descriptors independently of hidden Home shortcuts', () => {
  const photo = item('photo');
  const video = item('video', 'video/mp4');
  const voice = item('voice', 'audio/mp4');
  const pdf = item('pdf', 'application/pdf');
  const app = item('deck', 'application/octet-stream', {
    app: { app_id: 'deckcraft' } as ItemDetail['app'],
    kind_id: 'app',
  });
  const rows = [
    photo,
    video,
    voice,
    pdf,
    app,
    item('other-space', 'image/png', { space_id: 'secret' }),
  ];
  assert.deepEqual(sharedItems(rows, 'space', 'media'), [photo, video]);
  assert.deepEqual(sharedItems(rows, 'space', 'audio'), [voice]);
  assert.deepEqual(sharedItems(rows, 'space', 'apps'), [app]);
  assert.equal(sharedItems(rows, 'space', 'media')[0]!.version, 4);
  assert.equal(sharedItems(rows, 'space', 'media')[0]!.file!.sha256, photo.file!.sha256);
  assert.equal(sharedMediaTab(['apps']), 'media');
  assert.equal(sharedMediaTab('apps'), 'apps');
});
test('shared links omit deleted messages, malformed/credential URLs and deduplicate within each accepted message', () => {
  const rows = sharedLinks([
    message('old', 'https://example.test/a'),
    message('gone', 'https://secret.test', { deleted: true }),
    message(
      'new',
      'https://user:password@example.test/no https://example.test/a https://example.test/a https://[broken',
    ),
  ]);
  assert.deepEqual(
    rows.map((row) => [row.messageId, row.url]),
    [
      ['new', 'https://example.test/a'],
      ['old', 'https://example.test/a'],
    ],
  );
});
test('materialized media must match exact Item/version/hash/MIME; unexpected copies are released', async () => {
  const f = fixture('tile');
  const materialize = f.exports.materializeSharedFile;
  for (const mismatch of [
    { itemId: 'other' },
    { version: 3 },
    { info: { bytes: '5', mime: 'image/jpeg', sha256: 'wrong' } },
    { info: { bytes: '5', mime: 'video/mp4', sha256: 'aa'.repeat(32) } },
    { info: { bytes: '9', mime: 'image/jpeg', sha256: 'aa'.repeat(32) } },
  ]) {
    const accepted = {
      uri: 'local:unexpected',
      itemId: 'photo',
      version: 4,
      info: { bytes: '5', mime: 'image/jpeg', sha256: 'aa'.repeat(32) },
    };
    const releases: string[] = [];
    await assert.rejects(
      materialize(
        {
          materializeFile: async () => ({ ...accepted, ...mismatch }),
          releaseLocalFile: async (uri: string) => {
            releases.push(uri);
          },
        },
        item('photo'),
      ),
    );
    assert.deepEqual(releases, ['local:unexpected']);
  }
  const exact = await materialize(f.store.driver.media, item('photo'));
  assert.equal(exact.version, 4);
  assert.equal(exact.uri, 'local:photo');
});
test('late thumbnail after retirement releases its local copy; late Open never enters a foreign native preview', async () => {
  const f = fixture('tile');
  const thumbnail = deferred<any>();
  const opened = deferred<any>();
  let reads = 0;
  f.store.driver.media.materializeFile = () => (reads++ === 0 ? thumbnail.promise : opened.promise);
  let current = true;
  const tree = f.render({ item: item('photo'), owner: 'owner', isCurrent: () => current });
  tree.props.onPress();
  current = false;
  f.unmount();
  const local = {
    itemId: 'photo',
    version: 4,
    info: { bytes: '5', mime: 'image/jpeg', sha256: 'aa'.repeat(32) },
  };
  thumbnail.resolve({ ...local, uri: 'local:thumbnail' });
  opened.resolve({ ...local, uri: 'local:opened' });
  await settle();
  assert.equal(f.previews.length, 0);
  assert.deepEqual(f.releases.sort(), ['local:opened', 'local:thumbnail']);
});

test('Shared Media renders canonical space items and retires captured navigation when owner changes', () => {
  const routes: unknown[] = [];
  const f = fixture('shared', {
    'expo-router': {
      router: { push: (route: unknown) => routes.push(route) },
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'space' }),
    },
  });
  const deck = item('deck', 'application/octet-stream', {
    app: { app_id: 'deckcraft' } as ItemDetail['app'],
  });
  f.setQuery({
    data: [item('photo'), item('secret', 'image/png', { space_id: 'elsewhere' }), deck],
    error: null,
    retry() {},
  });
  const wrapper = f.render();
  const screen = wrapper.type as Handler;
  let tree = f.render(wrapper.props, screen);
  let list = nodes(tree).find((node) => node.type === 'FlashList')!;
  assert.deepEqual(
    Array.from(list.props.data, (row: ItemDetail) => row.id),
    ['photo'],
  );
  target(list.props.ListHeaderComponent, 'shared-media-tab-apps').props.onPress();
  tree = f.render(wrapper.props, screen);
  list = nodes(tree).find((node) => node.type === 'FlashList')!;
  assert.deepEqual(
    Array.from(list.props.data, (row: ItemDetail) => row.id),
    ['deck'],
  );
  const open = list.props.renderItem({ item: deck });
  open.props.onPress();
  assert.equal(routes.length, 1);
  f.store.sessionEpoch++;
  open.props.onPress();
  assert.equal(routes.length, 1);
});
test('Shared links use explicit canonical history/older requests and reject external open after owner retirement', async () => {
  const routes: unknown[] = [];
  const reads: unknown[] = [];
  const f = fixture('shared', {
    'expo-router': {
      router: { dismissTo: (route: unknown) => routes.push(route) },
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'space', tab: 'links' }),
    },
    'react-native': {
      View: 'View',
      Text: 'Text',
      ScrollView: 'ScrollView',
      Pressable: 'Pressable',
      Linking: {
        openURL: async (url: string) => {
          routes.push(url);
        },
      },
    },
  });
  f.state.timelines.space = {
    entries: [message('message', 'https://example.test/story')],
    has_more: true,
  };
  f.store.loadTimeline = async (...args: unknown[]) => {
    reads.push(args);
  };
  const wrapper = f.render();
  const tree = f.render(wrapper.props, wrapper.type as Handler);
  const list = nodes(tree).find((node) => node.type === 'FlashList')!;
  await settle();
  assert.deepEqual(reads, [['space']]);
  nodes(list.props.ListFooterComponent)
    .find((node) => node.type === 'NativeButton')!
    .props.onPress();
  await settle();
  assert.deepEqual(reads, [['space'], ['space', true]]);
  const row = list.props.renderItem({ item: list.props.data[0] });
  const external = nodes(row).find((node) => node.props.accessibilityRole === 'link')!;
  const jump = nodes(row).find((node) => node.props.accessibilityRole === 'button')!;
  external.props.onPress();
  jump.props.onPress();
  assert.equal(routes.length, 2);
  f.store.sessionEpoch++;
  external.props.onPress();
  jump.props.onPress();
  assert.equal(routes.length, 2);
  const earlier = sharedLinks([
    message('old', 'https://old.test'),
    message('message', 'https://example.test/story'),
  ]);
  const later = sharedLinks([
    message('old', 'https://old.test'),
    message('message', 'https://example.test/story'),
    message('new', 'https://new.test'),
  ]);
  assert.equal(
    earlier.find((link) => link.messageId === 'message')!.id,
    later.find((link) => link.messageId === 'message')!.id,
  );
});

test('admitted profile save may settle after blur without popping a later route or disposing its owned photo early', async () => {
  const f = fixture('edit');
  let disposed = 0;
  f.setPicker(async () => ({
    uri: 'local:owned',
    dispose() {
      disposed++;
    },
  }));
  target(f.render(), 'edit-profile-photo').props.onPress();
  await settle();
  const pending = deferred<void>();
  f.store.driver.photos.save = async (request) => {
    f.saves.push(request);
    await pending.promise;
  };
  const tree = f.render();
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  assert.equal(f.saves.length, 1);
  f.blur();
  assert.equal(disposed, 0);
  pending.resolve();
  await settle();
  assert.equal(f.routes.length, 0);
  assert.equal(disposed, 0);
  f.unmount();
  assert.equal(disposed, 1);
});
test('photo picker completion after route blur releases without replacing the retained profile draft', async () => {
  const f = fixture('edit');
  const selected = deferred<any>();
  let disposed = 0;
  f.setPicker(() => selected.promise);
  target(f.render(), 'edit-profile-photo').props.onPress();
  f.blur();
  selected.resolve({
    uri: 'local:late',
    dispose() {
      disposed++;
    },
  });
  await settle();
  assert.equal(disposed, 1);
  assert.equal(f.saves.length, 0);
  assert.equal(f.routes.length, 0);
});
