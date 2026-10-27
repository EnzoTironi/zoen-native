import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { ClientStore } from './store.ts';
import type { Command, CommandResult, CoreDriver, CoreEvent, Snapshot } from './types.ts';
import { allowLocalNotification } from '../notifications/local-admission.ts';
import { validLocalNotification } from '../notifications/links.ts';
import { NotificationJobs } from '../notifications/jobs.ts';
import { backgroundPresentationJobs } from '../notifications/presentation.ts';
import { nativeNotificationPlatform } from '../notifications/native-adapter.ts';
import { pushPlatform } from '../notifications/configuration.ts';
import { openSurfaceInvocation, type SurfaceInvocation } from '../native-surfaces/intents.ts';
import { surfaceOwner, surfaceSpaces } from '../native-surfaces/access.ts';

type Element = { type: unknown; props: Record<string, unknown> };
const entries = {
  notifications: '../notifications/provider.tsx',
  intents: '../native-surfaces/intent-consumer.ios.tsx',
} as const;
const compiled = Object.fromEntries(
  await Promise.all(
    Object.entries(entries).map(async ([name, path]) => {
      const output = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-navigation-ports',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path: dependency }) => ({
                path: dependency,
                external: true,
              }));
            },
          },
        ],
      });
      return [name, output.outputFiles[0]!.text];
    }),
  ),
);

const person = {
  id: 'owner',
  name: 'Synthetic',
  handle: 'synthetic',
  initials: 'S',
  kind: 'Person' as const,
  tint_hex: '#333333',
  glyph: null,
  bio: '',
  owner_id: null,
  owner_name: null,
  owner_tint_hex: null,
  photo_sha256: null,
  photo_ready: false,
  photo_mime: null,
  is_me: true,
  is_mine: false,
};
const snapshot: Snapshot = {
  account: {
    identity_id: person.id,
    device_id: 'device',
    name: person.name,
    handle: person.handle,
    relay_url: 'https://relay.example',
    registered: true,
    unlocked: true,
    setup_pending: false,
  },
  me: person,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
  conversations: [
    {
      id: 'chat',
      title: 'Synthetic',
      kind: 'Group',
      privacy: 'EndToEnd',
      can_write: true,
      members: [person],
      counterpart: null,
      last_preview: '',
      last_author: null,
      last_at_ms: '0',
      unread: 0,
      last_delivery: null,
      pinned: false,
      archived: false,
      muted: false,
      muted_until_ms: null,
      pending_requests: 0,
      event_count: '1',
    },
  ],
};
class Driver implements CoreDriver {
  calls: string[] = [];
  async open() {
    return snapshot;
  }
  async close() {}
  async createAccount(): Promise<Snapshot> {
    throw new Error('No new account during navigation');
  }
  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    this.calls.push(command.op);
    return snapshot as CommandResult<C>;
  }
  subscribe(_listener: (event: CoreEvent) => void) {
    return () => {};
  }
  async eraseDevice() {}
}

function mount(name: keyof typeof entries, dependencies: Record<string, unknown>) {
  const effects: (() => void | (() => void))[] = [];
  const jsx = (type: unknown, props: Element['props']) => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      createContext: () => 'Context',
      useMemo: (read: () => unknown) => read(),
      useEffect: (read: () => void | (() => void)) => effects.push(read),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    '../i18n/index.ts': { t: (key: string) => key },
    ...dependencies,
  };
  const module = { exports: {} as Record<string, (props?: Record<string, unknown>) => Element> };
  runInNewContext(compiled[name], {
    module,
    exports: module.exports,
    require(path: string) {
      assert.ok(Object.hasOwn(ports, path), `unmocked native import: ${path}`);
      return ports[path];
    },
  });
  const render = (
    name === 'notifications'
      ? module.exports.NotificationProvider
      : module.exports.NativeIntentConsumer
  )({ children: null });
  if (typeof render.type === 'function') render.type(render.props);
  const cleanups = effects.map((effect) => effect());
  return () => {
    for (const cleanup of cleanups.reverse()) cleanup?.();
  };
}

for (const platform of ['ios', 'android'])
  test(`${platform} actual notification handler leaves a guarded editor untouched and explains manual navigation`, async () => {
    const driver = new Driver();
    const store = new ClientStore(driver);
    await store.open();
    const before = store.getSnapshot();
    const epoch = store.sessionEpoch;
    const routes: unknown[] = [];
    const dialogs: unknown[] = [];
    let response: ((data: unknown) => void) | undefined;
    let dismiss!: () => void;
    let checks = 0;
    const allow = store.registerSessionPreflight(() => {
      checks++;
      throw new Error('Unsaved drawing');
    });
    class Coordinator {
      activate() {}
      dispose() {}
      async foreground() {}
      async receivedToken() {}
      getSnapshot() {
        return { permission: 'denied' };
      }
    }
    const unmount = mount('notifications', {
      '../core/provider': { useClient: () => ({ ...store.getSnapshot(), store }) },
      '../components/dialogs': {
        useDialogs: () => ({
          inform: (title: string, message: string) => {
            dialogs.push({ title, message });
            return new Promise<void>((resolve) => {
              dismiss = resolve;
            });
          },
        }),
      },
      'expo-router': { router: { push: (path: unknown) => routes.push(path) } },
      'react-native': {
        Platform: { OS: platform },
        AppState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
      },
      './coordinator': { PushCoordinator: Coordinator },
      './links': { validLocalNotification },
      './local-admission': { allowLocalNotification },
      './native-adapter': { nativeNotificationPlatform },
      './configuration': { pushPlatform },
      './presentation': { backgroundPresentationJobs },
      './jobs': { NotificationJobs },
      './system': {
        clearDeviceNotifications: async () => {},
        installNotificationHandler: () => () => {},
        listenNotificationEvents: (_wake: unknown, received: typeof response) => {
          response = received;
          return () => {};
        },
        listenPushToken: () => () => {},
        nativePushConfiguration: () => null,
        nativePushToken: async () => '',
        nativeNotificationPresentation: () => false,
        requestNativeNotes: async () => assert.fail('unexpected background presentation'),
        notificationPermission: async () => 'denied',
        showLocalNote: async () => {},
        updateBadge: async () => {},
      },
    });
    assert.ok(response);
    const data = { zoen_local: 1, device_id: 'device', space_id: 'chat', event_seq: '1' };
    response({ ...data, device_id: 'another-device' });
    assert.equal(checks, 0);
    response(data);
    assert.equal(checks, 1);
    assert.deepEqual(routes, []);
    assert.deepEqual(dialogs, [{ title: 'drawing.pending', message: 'drawing.closeFirst' }]);
    assert.equal(store.getSnapshot(), before);
    assert.equal(store.sessionEpoch, epoch);
    assert.deepEqual(driver.calls, []);
    allow();
    dismiss();
    await Promise.resolve();
    assert.deepEqual(routes, [], 'dismissing the explanation never replays the old click');
    response(data);
    assert.equal(routes.length, 1);
    assert.equal(JSON.stringify(routes[0]), '{"pathname":"/chat/[id]","params":{"id":"chat"}}');
    unmount();
    await store.close();
  });

test('actual native intent consumer retains a blocked invocation and opens it only on a later retry', async () => {
  const driver = new Driver();
  const store = new ClientStore(driver);
  await store.open();
  const before = store.getSnapshot();
  const epoch = store.sessionEpoch;
  const routes: string[] = [];
  const removed: string[] = [];
  let receive: ((pending: SurfaceInvocation[]) => Promise<void>) | undefined;
  const unmount = mount('intents', {
    '../core/provider': { useClient: () => ({ ...store.getSnapshot(), store }) },
    'expo-router': { router: { push: (path: string) => routes.push(path) } },
    'expo-app-intents': {
      useAppIntents: (callback: typeof receive) => {
        receive = callback;
      },
      removePendingInvocationAsync: async (id: string) => {
        removed.push(id);
      },
    },
    './intents': { openSurfaceInvocation },
    './platform': { surfacesAvailable: () => true },
    './access': { surfaceOwner, surfaceSpaces },
  });
  assert.ok(receive);
  const invocation: SurfaceInvocation = {
    id: 'synthetic-intent',
    name: 'zoen.openChat',
    params: { identityId: 'owner', deviceId: 'device', spaceId: 'chat' },
    createdAt: Date.now(),
  };
  // Membership validation yields before navigation. An editor admitted in that gap must win.
  const pending = receive([invocation]);
  const allow = store.registerSessionPreflight(() => {
    throw new Error('Unsaved drawing');
  });
  await pending;
  assert.deepEqual(routes, []);
  assert.deepEqual(removed, [], 'unavailable must retain the native invocation');
  assert.equal(store.getSnapshot(), before);
  assert.equal(store.sessionEpoch, epoch);
  assert.deepEqual(driver.calls, []);
  allow();
  await receive([invocation]);
  assert.deepEqual(routes, ['/chat/chat']);
  assert.deepEqual(removed, [invocation.id]);
  unmount();
  await store.close();
});
