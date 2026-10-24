import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as flowModule from '../profile/blocked-people.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { settings } from '../i18n/catalogs/settings.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { ownerPolicy } from '../i18n/catalogs/owner-policy.ts';
import { telegramPresentation } from '../theme/telegram.ts';

type Element = { type: any; props: Record<string, any> };
const files = {
  hook: '../profile/use-blocked-people.ts',
  view: '../profile/blocked-people-view.tsx',
  route: '../app/settings/blocked-people.tsx',
  you: '../app/(tabs)/you.tsx',
};
const compiled = new Map(
  await Promise.all(
    Object.entries(files).map(async ([name, path]) => {
      const output = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'blocked-people-native-ports',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [name, output.outputFiles[0]!.text] as const;
    }),
  ),
);
const first = 'a'.repeat(64),
  second = 'b'.repeat(64);
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (value: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function settle() {
  for (let n = 0; n < 20; ++n) await Promise.resolve();
}
const plain = (value: unknown) => JSON.parse(JSON.stringify(value));
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
const strings = (tree: unknown): string => {
  if (typeof tree === 'string') return tree;
  if (Array.isArray(tree)) return tree.map(strings).join(' ');
  return tree && typeof tree === 'object' && 'props' in tree
    ? strings((tree as Element).props.children)
    : '';
};
function fixture(locale: 'en' | 'pt-BR' = 'en') {
  const localization = createLocalization({ settings, chat, ownerPolicy }, [locale]);
  const state: any = {
    phase: 'ready',
    erasePending: false,
    mode: 'account',
    snapshot: {
      account: {
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: 'https://relay.example/',
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
      me: { id: 'owner', name: 'Me' },
    },
  };
  const calls: any[] = [],
    routes: any[] = [],
    dialogs: any[] = [];
  const listeners = new Set<() => void>(),
    preflight = new Set<() => void>();
  const foreground = new Set<(value: string) => void>();
  let ids = [first, second];
  let rpc: (command: any) => Promise<any> = async (command) => {
    if (command.op === 'blocked_people') return [...ids];
    if (command.op === 'profile')
      return {
        identity_id: command.identity_id,
        handle: 'nina',
        name: 'Nina',
        photo_sha256: null,
        photo_ready: false,
        tint_hex: '#667788',
        is_me: false,
      };
    if (command.op === 'unblock_person') {
      ids = ids.filter((id) => id !== command.identity_id);
      return;
    }
    throw new Error('Unexpected operation');
  };
  const request = async (command: any) => {
    calls.push(plain(command));
    return rpc(command);
  };
  const store: any = {
    getSnapshot: () => state,
    sessionEpoch: 1,
    isSessionRetiring: false,
    driver: { request },
    request,
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    registerSessionPreflight: (fn: () => void) => {
      preflight.add(fn);
      return () => {
        preflight.delete(fn);
      };
    },
  };
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
  let confirm: (options: unknown) => Promise<boolean> = async () => false;
  let cursor = 0;
  const slots: any[] = [];
  const memo = (factory: () => any, deps: unknown[]) => {
    const index = cursor++;
    if (!slots[index] || !deps.every((value, i) => Object.is(value, slots[index].deps[i])))
      slots[index] = { value: factory(), deps };
    return slots[index].value;
  };
  let focused: (() => void | (() => void)) | undefined;
  let nextFocus: typeof focused;
  let cleanup: (() => void) | undefined;
  const loaded = new Map<string, any>();
  const require = (name: string): any => {
    if (name === 'react')
      return {
        useMemo: memo,
        useCallback: (fn: () => any, deps: unknown[]) => memo(() => fn, deps),
        useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
        useEffect: (fn: () => unknown) => fn(),
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
        FlatList: 'FlatList',
        View: 'View',
        Text: 'Text',
        Pressable: 'Pressable',
        ActivityIndicator: 'ActivityIndicator',
        ScrollView: 'ScrollView',
      };
    if (name === 'expo-router')
      return {
        Stack: { Screen: 'Stack.Screen' },
        router: {
          push: (route: unknown) => {
            routes.push(plain(route));
          },
        },
        useFocusEffect: (fn: typeof focused) => {
          if (fn !== focused) nextFocus = fn;
        },
      };
    if (name.endsWith('/core/provider')) return { useClient: () => ({ ...state, store }) };
    if (name.endsWith('/i18n') || name.endsWith('/i18n/index.ts')) return localization;
    if (name.endsWith('/theme'))
      return {
        useTheme: () => ({
          presentation: telegramPresentation('day'),
          colors: { text: '#111111' },
        }),
      };
    if (name === './blocked-people') return flowModule;
    if (name.endsWith('/use-blocked-people')) return load('hook');
    if (name.endsWith('/blocked-people-view')) return load('view');
    if (name.endsWith('/use-profile-photo')) return { useProfilePhoto: () => undefined };
    if (name.endsWith('/components/dialogs'))
      return {
        useDialogs: () => ({
          confirm: (options: unknown) => {
            dialogs.push(plain(options));
            return confirm(options);
          },
        }),
      };
    if (name.endsWith('/components/primitives'))
      return { NativeButton: 'NativeButton', Avatar: 'Avatar', EmptyState: 'EmptyState' };
    if (name.endsWith('/components/sky-background')) return { SkyScene: 'SkyScene' };
    if (name.endsWith('/components/settings-rows')) return { SettingsRows: 'SettingsRows' };
    if (name.endsWith('/profile/profile-header')) return { ProfileHeader: 'ProfileHeader' };
    if (name.endsWith('/profile/profile-section'))
      return { ProfileSection: 'ProfileSection', ProfileInfoRow: 'ProfileInfoRow' };
    if (name.endsWith('/integrity/integrity-warning'))
      return { IntegrityWarnings: 'IntegrityWarnings' };
    throw new Error(`Unexpected platform port: ${name}`);
  };
  function load(name: string): any {
    if (loaded.has(name)) return loaded.get(name);
    const module = { exports: {} };
    runInNewContext(compiled.get(name)!, { module, require, process: { env: { EXPO_OS: 'ios' } } });
    loaded.set(name, module.exports);
    return module.exports;
  }
  const render = (name = 'hook') => {
    cursor = 0;
    const result = name === 'hook' ? load(name).useBlockedPeople() : load(name).default();
    if (nextFocus) {
      cleanup?.();
      focused = nextFocus;
      nextFocus = undefined;
      cleanup = focused!() || undefined;
    }
    return result;
  };
  const route = () => {
    const tree = render('route');
    const view = nodes(tree).find((node) => node.type === load('view').BlockedPeopleView)!;
    return { props: view.props, list: view.type(view.props) as Element };
  };
  return {
    state,
    store,
    calls,
    routes,
    dialogs,
    render,
    route,
    rpc: (fn: typeof rpc) => {
      rpc = fn;
    },
    confirm: (fn: typeof confirm) => {
      confirm = fn;
    },
    changed: () => {
      for (const fn of listeners) fn();
    },
    preflight: () => {
      for (const fn of preflight) fn();
    },
    appState: (value: string) => {
      appState.currentState = value;
      for (const fn of foreground) fn(value);
    },
    close: () => {
      cleanup?.();
      cleanup = undefined;
    },
    refocus: () => {
      cleanup = focused?.() || undefined;
    },
    counts: () => ({
      listeners: listeners.size,
      preflight: preflight.size,
      foreground: foreground.size,
    }),
  };
}

test('actual hook reads on focus/foreground and immediately retires on background, preflight and device/relay replacement', async () => {
  const f = fixture();
  f.render();
  await settle();
  const session = f.render().session;
  assert.equal(session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.calls, [{ op: 'blocked_people' }]);
  f.appState('inactive');
  assert.equal(session.getSnapshot().phase, 'inactive');
  f.appState('active');
  await settle();
  assert.equal(f.calls.length, 2);
  f.state.snapshot.account.device_id = 'new-device';
  f.changed();
  assert.equal(session.getSnapshot().phase, 'inactive');
  f.render();
  await settle();
  assert.equal(f.render().state.phase, 'ready');
  f.state.snapshot.account.relay_url = 'https://relay.example';
  f.changed();
  assert.equal(session.getSnapshot().phase, 'inactive');
  f.render();
  await settle();
  f.preflight();
  assert.equal(session.getSnapshot().phase, 'inactive');
  f.close();
  assert.deepEqual(f.counts(), { listeners: 0, preflight: 0, foreground: 0 });
  const reads = f.calls.length;
  f.refocus();
  await settle();
  assert.equal(f.calls.length, reads + 1);
  assert.equal(session.getSnapshot().phase, 'ready');
  f.close();
});

test('real route passes exact selected identity, shows confirmation and reconciles Core; stale rendered callbacks cannot reopen or mutate', async () => {
  const f = fixture();
  f.route();
  await settle();
  let view = f.route();
  const session = view.props.session;
  assert.equal(view.list.type, 'FlatList');
  assert.equal(view.list.props.data.length, 2);
  assert.equal(
    f.calls.some((call) => call.op === 'profile'),
    false,
  );
  const row = view.list.props.renderItem({ item: view.list.props.data[0] });
  row.type(row.props);
  await settle();
  view = f.route();
  assert.deepEqual(f.calls[1], { op: 'profile', identity_id: first });
  view.props.onOpen(first);
  assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: first } }]);
  const answer = deferred<boolean>();
  f.confirm(() => answer.promise);
  view.props.onUnblock(session.getSnapshot().people[0]);
  assert.match(f.dialogs[0].message, /Nina/);
  assert.equal(
    f.calls.some((call) => call.op === 'unblock_person'),
    false,
  );
  answer.resolve(true);
  await settle();
  assert.deepEqual(
    f.calls.filter((call) => call.op === 'unblock_person'),
    [{ op: 'unblock_person', identity_id: first }],
  );
  assert.deepEqual(
    plain(f.route().list.props.data).map((person: any) => person.id),
    [second],
  );
  const retained = f.route().props;
  f.state.snapshot.account.device_id = 'replacement';
  f.changed();
  f.render();
  await settle();
  retained.onUnblock({ id: second, profile: null, phase: 'idle' });
  retained.onOpen(second);
  await settle();
  assert.equal(f.dialogs.length, 1);
  assert.equal(f.routes.length, 1);
});

test('late accepted dialog after background/owner change never dispatches; a rejected read never enables actions', async () => {
  const f = fixture();
  f.route();
  await settle();
  const view = f.route();
  const answer = deferred<boolean>();
  f.confirm(() => answer.promise);
  view.props.onUnblock(view.list.props.data[0]);
  f.appState('background');
  answer.resolve(true);
  await settle();
  assert.equal(
    f.calls.some((call) => call.op === 'unblock_person'),
    false,
  );
  f.rpc(async () => {
    throw new Error('synthetic read failure');
  });
  f.appState('active');
  await settle();
  const failed = f.route();
  assert.equal(failed.props.state.readFailed, true);
  assert.equal(nodes(failed.list.props.ListFooterComponent)[1]!.props.label, 'Try again');
  failed.props.onUnblock(view.list.props.data[0]);
  await settle();
  assert.equal(f.dialogs.length, 1);
});

test('PT/EN actual list states and recovered profile failures show human labels, never technical IDs', async () => {
  for (const [locale, name, empty] of [
    ['en', 'Blocked person', 'No blocked people'],
    ['pt-BR', 'Pessoa bloqueada', 'Nenhuma pessoa bloqueada'],
  ] as const) {
    const f = fixture(locale);
    f.route();
    await settle();
    f.rpc(async (command) => {
      if (command.op === 'profile') throw new Error('unreadable profile');
      return [first];
    });
    let view = f.route();
    let row = view.list.props.renderItem({ item: view.list.props.data[0] });
    row.type(row.props);
    await settle();
    view = f.route();
    row = view.list.props.renderItem({ item: view.list.props.data[0] });
    const content = row.type(row.props);
    assert.ok(!strings(content).includes(first));
    assert.ok(strings(content).includes(name));
    f.rpc(async () => []);
    nodes(view.list.props.ListFooterComponent)
      .find((node) => node.props.testID === 'blocked-people-refresh')!
      .props.onPress();
    await settle();
    assert.equal(f.route().list.props.ListEmptyComponent.props.title, empty);
  }
});

test('You exposes recovery in the real privacy section only for an account', () => {
  const f = fixture();
  let tree = f.render('you');
  tree = tree.props.children.type();
  const section = nodes(tree).find((node) => node.props.testID === 'you-privacy-rows')!;
  assert.equal(section.props.rows[0].title, 'Blocked people');
  section.props.rows[0].onPress();
  assert.deepEqual(f.routes, ['/settings/blocked-people']);
  f.state.mode = 'demo';
  tree = f.render('you');
  tree = tree.props.children.type();
  assert.equal(
    nodes(tree).some((node) => node.props.testID === 'you-privacy-rows'),
    false,
  );
});
