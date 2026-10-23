import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { build } from 'esbuild';

type State = 'active' | 'background' | 'inactive';
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

function fixture(initial: State, platform = 'ios') {
  const calls: Call[] = [];
  const effects: (() => unknown)[] = [];
  const cleanups: (() => void)[] = [];
  const changes = new Set<(state: State) => void>();
  let current = {
    mode: 'account',
    snapshot: {
      account: { identity_id: 'synthetic-owner', device_id: 'synthetic-device' },
      connection: { synced: true },
      conversations: [],
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
  const store = {
    getSnapshot: () => current,
    registerDeviceCleanup: () => () => {},
    subscribe: () => () => {},
    assertExternalNavigationAllowed() {},
    refresh: async () => {},
    driver: {
      async request(value: unknown) {
        calls.push({ kind: 'register', value });
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
    listenNotificationEvents: () => () => {},
    installNotificationHandler: () => () => {},
    nativeNotificationPresentation: () => true,
    requestNativeNotes: async () => {},
    showLocalNote: async () => {},
    updateBadge: async () => {},
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
    'expo-router': { router: { push: () => assert.fail('unexpected navigation') } },
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
    async flush() {
      // Ports above resolve immediately. One event-loop turn drains their real
      // coordinator continuations without sleeping or changing its deadlines.
      await new Promise<void>((resolve) => setImmediate(resolve));
    },
    enter(state: State) {
      appState.currentState = state;
      for (const callback of changes) callback(state);
    },
    replaceAccount() {
      current = {
        ...current,
        snapshot: {
          ...current.snapshot,
          account: { identity_id: 'other-owner', device_id: 'other-device' },
        },
      };
    },
    close() {
      for (const cleanup of cleanups) cleanup();
    },
    listenerCount: () => changes.size,
  };
}

test('background and inactive provider mounts do not start registration; active transitions retain the normal path', async () => {
  for (const initial of ['background', 'inactive'] as const) {
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
          value: { op: 'register_push', platform: 'apns-sandbox', token: 'ab12' },
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
