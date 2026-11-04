import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { build } from 'esbuild';
import type { SpaceSummary } from './generated/SpaceSummary';
import { mutedSnapshot } from './muted-chats.fixtures.ts';

type State = 'active' | 'background' | 'inactive' | 'unknown' | null;
type Call = { kind: string; value?: unknown };

// Compile the actual provider and real registration coordinator. Only React's
// mount/unmount hooks and external OS/client ports are controlled by this fixture.
const providerPath = new URL('../notifications/provider.tsx', import.meta.url).pathname;
const compiled = await build({
  entryPoints: [providerPath],
  bundle: true,
  write: false,
  format: 'cjs',
  platform: 'node',
  jsx: 'automatic',
  plugins: [
    {
      name: 'provider-effect-ports',
      setup(builder) {
        builder.onResolve({ filter: /.*/ }, (args) => {
          if (['react', 'react/jsx-runtime', 'react-native', 'expo-router'].includes(args.path))
            return { path: args.path, external: true };
          if (args.path === '../i18n' || args.path === '../i18n/index.ts')
            return { path: 'fixture:i18n', external: true };
          if (args.importer !== providerPath) return;
          const ports: Record<string, string> = {
            '../core/provider': 'fixture:client',
            '../components/dialogs': 'fixture:dialogs',
            './system': 'fixture:system',
          };
          const path = ports[args.path];
          return path ? { path, external: true } : undefined;
        });
      },
    },
  ],
});

function fixture(initial: State, platform = 'ios', failFirstRegistration = false) {
  const calls: Call[] = [];
  const effects: (() => unknown)[] = [];
  const cleanups: (() => void)[] = [];
  const storeListeners = new Set<() => void>();
  const changes = new Set<(state: State) => void>();
  let current = {
    phase: 'ready',
    mode: 'account',
    snapshot: {
      account: {
        identity_id: 'synthetic-owner',
        device_id: 'synthetic-device',
        relay_url: 'https://relay.example',
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
      connection: { synced: true },
      conversations: [] as SpaceSummary[],
    },
    erasePending: false,
  };
  const appState = {
    currentState: initial,
    addEventListener(event: string, callback: (state: State) => void) {
      assert.equal(event, 'change');
      changes.add(callback);
      return { remove: () => changes.delete(callback) };
    },
  };
  let present: (data: unknown) => Promise<boolean> = async () => false;
  let respond: (data: unknown) => void = () => {};
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => current,
    registerDeviceCleanup: () => () => {},
    subscribe: (fn: () => void) => {
      storeListeners.add(fn);
      return () => storeListeners.delete(fn);
    },
    assertExternalNavigationAllowed() {},
    refresh: async () => {},
    driver: {
      async request(value: unknown) {
        if ((value as { op?: string }).op === 'snapshot') {
          calls.push({ kind: 'snapshot' });
          return current.snapshot;
        }
        calls.push({ kind: 'register', value });
        if (failFirstRegistration) {
          failFirstRegistration = false;
          throw new Error('offline');
        }
      },
    },
  };
  const system = {
    nativePushConfiguration: () => ({ environment: 'sandbox', topic: 'xyz.tironi.zoen' }),
    async notificationPermission(request: boolean) {
      calls.push({ kind: 'permission', value: request });
      return 'allowed';
    },
    async nativePushToken() {
      calls.push({ kind: 'token' });
      return 'ab12';
    },
    listenPushToken: () => () => {},
    listenNotificationEvents: (_wake: unknown, onResponse: typeof respond) => {
      respond = onResponse;
      return () => {};
    },
    installNotificationHandler: (handler: typeof present) => {
      present = handler;
      return () => {};
    },
    nativeNotificationPresentation: () => true,
    requestNativeNotes: async () => {
      calls.push({ kind: 'nativeNote' });
    },
    showLocalNote: async () => {
      calls.push({ kind: 'localNote' });
    },
    updateBadge: async (value: number) => {
      calls.push({ kind: 'badge', value });
    },
    clearDeviceNotifications: async () => {},
  };
  const imports: Record<string, unknown> = {
    react: {
      createContext: () => ({}),
      useMemo: (callback: () => unknown) => callback(),
      useEffect: (callback: () => unknown) => effects.push(callback),
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: unknown) => ({ type, props }),
    },
    'react-native': { AppState: appState, Platform: { OS: platform } },
    'expo-router': {
      router: { push: (value: unknown) => calls.push({ kind: 'navigation', value }) },
    },
    'fixture:i18n': { t: (key: string) => key },
    'fixture:dialogs': { useDialogs: () => ({ inform: async () => {} }) },
    'fixture:client': {
      useClient: () => ({ store, snapshot: current.snapshot, mode: current.mode }),
    },
    'fixture:system': system,
  };
  const module = {
    exports: {} as { NotificationProvider: (props: unknown) => { props: { value: unknown } } },
  };
  vm.runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      assert.ok(name in imports, `unexpected provider import: ${name}`);
      return imports[name];
    },
    setTimeout,
    clearTimeout,
  });
  module.exports.NotificationProvider({ children: null });
  for (const mount of effects) {
    const cleanup = mount();
    if (typeof cleanup === 'function') cleanups.push(cleanup as () => void);
  }
  return {
    calls,
    respond,
    present,
    reopen() {
      store.sessionEpoch++;
    },
    retire() {
      store.isSessionRetiring = true;
    },
    lock() {
      current.snapshot.account.unlocked = false;
    },
    changeRelay() {
      current.snapshot.account.relay_url = 'https://other.example';
    },
    async flush() {
      // Ports above resolve immediately. One event-loop turn drains their real
      // coordinator continuations without sleeping or changing its deadlines.
      await new Promise<void>((resolve) => setImmediate(resolve));
    },
    enter(state: State) {
      appState.currentState = state;
      for (const callback of changes) callback(state);
    },
    synced(synced: boolean) {
      current = { ...current, snapshot: { ...current.snapshot, connection: { synced } } };
      for (const listener of storeListeners) listener();
    },
    replaceAccount() {
      current = {
        ...current,
        snapshot: {
          ...current.snapshot,
          account: {
            registered: true,
            identity_id: 'other-owner',
            device_id: 'other-device',
            relay_url: 'https://other.example',
            unlocked: true,
            setup_pending: false,
          },
        },
      };
    },
    close() {
      for (const cleanup of cleanups) cleanup();
    },
    listenerCount: () => changes.size,
    conversations: (value: SpaceSummary[]) => {
      current = { ...current, snapshot: { ...current.snapshot, conversations: value } };
      for (const listener of storeListeners) listener();
    },
  };
}

test('unknown, background and inactive provider mounts wait for activation before registration', async () => {
  for (const initial of [null, 'unknown', 'background', 'inactive'] as const) {
    const f = fixture(initial);
    try {
      await f.flush();
      assert.deepEqual(
        f.calls,
        [],
        `${initial} mount must not request a permission/token or register`,
      );
      f.enter('active');
      await f.flush();
      assert.deepEqual(JSON.parse(JSON.stringify(f.calls)), [
        { kind: 'permission', value: false },
        { kind: 'token' },
        {
          kind: 'register',
          value: {
            op: 'register_push',
            expected_identity_id: 'synthetic-owner',
            expected_device_id: 'synthetic-device',
            platform: 'apns-sandbox',
            token: 'ab12',
          },
        },
      ]);
    } finally {
      f.close();
    }
  }
});

test('already-active mount still registers and disposed listeners cannot restart it', async () => {
  const f = fixture('active');
  await f.flush();
  assert.deepEqual(
    f.calls.map((call) => call.kind),
    ['permission', 'token', 'register'],
  );
  assert.equal(f.listenerCount(), 1);
  f.close();
  assert.equal(f.listenerCount(), 0);
  f.enter('active');
  await f.flush();
  assert.equal(f.calls.length, 3);
});

test('a replaced account cannot register from an older provider activation', async () => {
  const f = fixture('background');
  try {
    await f.flush();
    f.calls.length = 0;
    f.replaceAccount();
    f.enter('active');
    await f.flush();
    assert.deepEqual(f.calls, []);
  } finally {
    f.close();
  }
});

test('web mount retains the non-native path without token work or an AppState listener', async () => {
  const f = fixture('active', 'web');
  try {
    await f.flush();
    assert.deepEqual(f.calls, []);
    assert.equal(f.listenerCount(), 0);
  } finally {
    f.close();
  }
});

test('manual unread contributes one badge unit without emitting a message alert or double-counting real unread', async () => {
  const f = fixture('active');
  try {
    await f.flush();
    f.enter('background');
    const chat = { ...mutedSnapshot().conversations[0]!, muted: false, archived: false, unread: 0 };
    chat.members = [{ ...chat.members[0]!, id: 'synthetic-owner' }];
    chat.last_author = { ...chat.members[0]!, id: 'sender', is_me: false };
    chat.event_count = '1';
    f.conversations([{ ...chat, unread: 1 }]);
    await f.flush();
    assert.ok(
      f.calls.some((call) => call.kind === 'nativeNote'),
      'The same live provider can present a real incoming message',
    );
    f.conversations([chat]);
    await f.flush();
    f.calls.length = 0;
    f.conversations([{ ...chat, manual_unread: true }]);
    await f.flush();
    assert.deepEqual(f.calls, [{ kind: 'badge', value: 1 }]);
    f.enter('active');
    await f.flush();
    f.calls.length = 0;
    f.conversations([{ ...chat, unread: 3, manual_unread: true }]);
    await f.flush();
    assert.deepEqual(f.calls, [{ kind: 'badge', value: 3 }]);
    f.calls.length = 0;
    f.conversations([
      { ...chat, id: 'muted', manual_unread: true, muted: true },
      { ...chat, id: 'archived', manual_unread: true, archived: true },
    ]);
    await f.flush();
    assert.deepEqual(f.calls, [{ kind: 'badge', value: 0 }]);
    f.calls.length = 0;
    f.conversations([{ ...chat, manual_unread: false }]);
    await f.flush();
    assert.deepEqual(f.calls, [{ kind: 'badge', value: 0 }]);
  } finally {
    f.close();
  }
});

test('foreground resync retries a waiting registration once', async () => {
  const f = fixture('active', 'ios', true);
  const registrations = () => f.calls.filter(({ kind }) => kind === 'register').length;
  try {
    await f.flush();
    assert.equal(registrations(), 1);
    f.synced(false);
    await f.flush();
    assert.equal(registrations(), 1);
    f.synced(true);
    f.synced(true);
    await f.flush();
    assert.equal(registrations(), 2);
  } finally {
    f.close();
  }
});

test('retired same-device notification callbacks cannot navigate or query presentation', async () => {
  const f = fixture('background');
  try {
    f.conversations([{ id: 'chat', members: [{ id: 'synthetic-owner' }] } as SpaceSummary]);
    const data = { zoen_local: 1, device_id: 'synthetic-device', event_seq: '1', space_id: 'chat' };
    f.respond(data);
    assert.equal(f.calls.filter(({ kind }) => kind === 'navigation').length, 1);
    f.reopen();
    assert.equal(await f.present(data), false);
    assert.equal(
      f.calls.some(({ kind }) => kind === 'register'),
      false,
    );
    f.respond(data);
    assert.equal(f.calls.filter(({ kind }) => kind === 'navigation').length, 1);
  } finally {
    f.close();
  }
});

test('notification taps require an unlocked live account on the original relay', () => {
  for (const invalidate of ['lock', 'retire', 'changeRelay'] as const) {
    const f = fixture('background');
    try {
      f.conversations([{ id: 'chat', members: [{ id: 'synthetic-owner' }] } as SpaceSummary]);
      f[invalidate]();
      f.respond({ zoen_local: 1, device_id: 'synthetic-device', event_seq: '1', space_id: 'chat' });
      assert.equal(
        f.calls.some(({ kind }) => kind === 'navigation'),
        false,
        invalidate,
      );
    } finally {
      f.close();
    }
  }
});
