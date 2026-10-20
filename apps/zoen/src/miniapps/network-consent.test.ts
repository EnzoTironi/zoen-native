import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { createNetworkConsent } from './network-consent.ts';
import { createDeviceConsentQueue, type ConsentChoice } from './device-consent-queue.ts';
import type { CoreDriver, Command, ItemDetail } from '../core/types';
import type { ClientStore } from '../core/store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function fixture() {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'identity-a',
        device_id: 'device-a',
        unlocked: true,
        setup_pending: false,
      },
      conversations: [{ id: 'space-a' }],
    },
  };
  const owner = {
    sessionEpoch: 1,
    getSnapshot: () => state as unknown as ReturnType<ClientStore['getSnapshot']>,
  };
  const item = {
    id: 'item-a',
    title: 'Actual maps fixture',
    space_id: 'space-a',
    space_title: 'Group',
    app: { app_id: 'maps' },
  } as ItemDetail;
  const spec = {
    id: 'maps',
    name: 'Maps',
    resource_uri: 'ui://builtin/maps',
    has_view: true,
    tools: [],
  };
  const text = '<!doctype html><main>Maps fixture · Árvore</main>';
  const resource = {
    uri: spec.resource_uri,
    mime_type: 'text/html;profile=mcp-app',
    text,
    prefers_border: false,
    manifest_json: JSON.stringify({
      id: 'maps',
      name: 'Maps',
      capabilities: [],
      allowedDomains: ['tiles.example.org'],
      linkDomains: [],
      networkPurpose: 'Load these exact map tiles',
      bytes: new TextEncoder().encode(text).length,
      sha256: createHash('sha256').update(text).digest('hex'),
    }),
  };
  const reads: Command[] = [];
  const writes: Command[] = [];
  const asked: string[] = [];
  const allowed = new Set<string>();
  const queue = createDeviceConsentQueue();
  const f = {
    state,
    owner,
    item,
    spec,
    resource,
    reads,
    writes,
    asked,
    allowed,
    queue,
    read: async (command: Command): Promise<unknown> => {
      reads.push(command);
      switch (command.op) {
        case 'item':
          return { ...item, app: { ...item.app } };
        case 'app_specs':
          return [{ ...spec }];
        case 'app_resource':
          return { ...resource };
        case 'app_device_allowed':
          return allowed.has(command.capability);
        default:
          throw Error(`Unexpected read ${command.op}`);
      }
    },
    ask: async (capability: string, purpose: string) => {
      asked.push(capability);
      return queue.ask(capability, purpose);
    },
    control: null as unknown as ReturnType<typeof createNetworkConsent>,
  };
  f.control = createNetworkConsent(item.id, {
    store: owner,
    read: ((command: Command) => f.read(command)) as CoreDriver['request'],
    write: (async (command: Command) => {
      assert.equal(command.op, 'grant_app_device');
      writes.push(command);
      if (command.op === 'grant_app_device') allowed.add(command.capability);
      return 'real-port-grant-id';
    }) as CoreDriver['request'],
    ask: (capability, purpose) => f.ask(capability, purpose),
    cancelConsent: queue.cancelAll,
  });
  return f;
}
async function pending(f: ReturnType<typeof fixture>) {
  for (let n = 0; n < 100; n++) {
    if (f.queue.getSnapshot().current) return f.queue.getSnapshot().current!;
    await new Promise(setImmediate);
  }
  assert.fail('Consent was not reached');
}

test('verified policy uses the real dismissal barrier, coalesces repeated activation, and grants exact host/purpose once', async () => {
  const f = fixture();
  const view = await f.control.read();
  assert.deepEqual(view?.manifest.allowedDomains, ['tiles.example.org']);
  const first = f.control.grant();
  const second = f.control.grant();
  const request = await pending(f);
  assert.equal(f.queue.choose(request.id, 'once'), true);
  await Promise.resolve();
  assert.equal(f.writes.length, 0);
  assert.equal(f.queue.dismissed(request.id), true);
  await Promise.all([first, second]);
  assert.equal(f.asked.length, 1);
  assert.deepEqual(f.writes, [
    {
      op: 'grant_app_device',
      item_id: 'item-a',
      capability: 'net:tiles.example.org',
      purpose: 'Load these exact map tiles',
      always: false,
    },
  ]);
});

test('tampered resource and wrong resource identity refuse before consent or any grant', async () => {
  for (const change of [
    (f: ReturnType<typeof fixture>) => {
      f.resource.text += 'tamper';
    },
    (f: ReturnType<typeof fixture>) => {
      f.resource.uri = 'ui://another/app';
    },
  ]) {
    const f = fixture();
    change(f);
    await assert.rejects(f.control.read());
    assert.equal(f.asked.length, 0);
    assert.equal(f.writes.length, 0);
  }
});

test('denial grants nothing and always requires a new explicit decision for retry', async () => {
  const f = fixture();
  await f.control.read();
  f.ask = async () => 'deny';
  await assert.rejects(f.control.grant());
  assert.equal(f.writes.length, 0);
  f.ask = async () => 'always';
  await f.control.grant();
  assert.equal(f.writes.length, 1);
  assert.equal((f.writes[0] as { always: boolean }).always, true);
});

test('blur, epoch, device, account, erase and membership changes while consent is held all refuse the later write', async () => {
  const changes = [
    (f: ReturnType<typeof fixture>) => f.control.close(),
    (f: ReturnType<typeof fixture>) => {
      f.owner.sessionEpoch++;
    },
    (f: ReturnType<typeof fixture>) => {
      f.state.snapshot.account.device_id = 'device-b';
    },
    (f: ReturnType<typeof fixture>) => {
      f.state.snapshot.account.identity_id = 'identity-b';
    },
    (f: ReturnType<typeof fixture>) => {
      f.state.erasePending = true;
    },
    (f: ReturnType<typeof fixture>) => {
      f.state.snapshot.conversations = [];
    },
  ];
  for (const change of changes) {
    const f = fixture();
    await f.control.read();
    const decision = deferred<ConsentChoice>();
    const entered = deferred<void>();
    f.ask = async () => {
      entered.resolve();
      return decision.promise;
    };
    const work = f.control.grant();
    await entered.promise;
    change(f);
    decision.resolve('once');
    await assert.rejects(work);
    assert.equal(f.writes.length, 0);
  }
});

test('a changed App binding or resource after approval is refused by the fresh pre-write read', async () => {
  for (const change of [
    (f: ReturnType<typeof fixture>) => {
      f.item.space_id = 'other-space';
    },
    (f: ReturnType<typeof fixture>) => {
      f.spec.resource_uri = 'ui://builtin/other';
    },
    (f: ReturnType<typeof fixture>) => {
      f.resource.text += 'changed after consent';
    },
  ]) {
    const f = fixture();
    await f.control.read();
    f.ask = async () => {
      change(f);
      return 'once';
    };
    await assert.rejects(f.control.grant());
    assert.equal(f.writes.length, 0);
  }
});

test('late resource responses cannot revive a closed route or authorize its replacement', async () => {
  const f = fixture();
  const result = deferred<unknown>();
  const entered = deferred<void>();
  const normal = f.read;
  f.read = async (command) =>
    command.op === 'app_resource' ? (entered.resolve(), result.promise) : normal(command);
  const loading = f.control.read();
  await entered.promise;
  f.control.close();
  result.resolve({ ...f.resource });
  await assert.rejects(loading);
  await assert.rejects(f.control.grant());
  assert.equal(f.writes.length, 0);
});

test('current Rust authorization is checked again; existing access never causes another prompt or grant', async () => {
  const f = fixture();
  f.allowed.add('net:tiles.example.org');
  const policy = await f.control.read();
  assert.deepEqual(policy?.allowedHosts, ['tiles.example.org']);
  await f.control.grant();
  assert.equal(f.asked.length, 0);
  assert.equal(f.writes.length, 0);
});

// Actual compiled route consumer, with React/native/controller ports. This is not native Auth/UI proof.
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const routePath = new URL('../app/miniapp-permissions.tsx', import.meta.url);
const consumer = await build({
  stdin: {
    contents:
      readFileSync(process.env.ZOEN_NETWORK_PERMISSION_SOURCE ?? routePath, 'utf8') +
      '\nexport { ItemNetworkAccess };',
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'consumer-ports',
      setup(builder) {
        builder.onResolve({ filter: /.*/ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
type Element = { type: unknown; props: Record<string, any> };
function routeFixture() {
  const slots: { value?: any; deps?: readonly unknown[] }[] = [];
  let cursor = 0;
  let focus: (() => void | (() => void)) | null = null;
  let committed: (() => void | (() => void)) | null = null;
  let cleanup: (() => void) | null = null;
  const scopes: { closed: boolean; done: ReturnType<typeof deferred<void>> }[] = [];
  const state = {
    mode: 'account',
    snapshot: { account: { identity_id: 'account-a', device_id: 'device-a' } },
  };
  const store = { sessionEpoch: 1 };
  const slot = () => (slots[cursor++] ??= {});
  const react = {
    useState(initial: unknown) {
      const s = slot();
      if (!Object.hasOwn(s, 'value')) s.value = initial;
      return [
        s.value,
        (next: any) => {
          s.value = typeof next === 'function' ? next(s.value) : next;
        },
      ];
    },
    useRef(initial: unknown) {
      const s = slot();
      return (s.value ??= { current: initial });
    },
    useLayoutEffect(effect: () => void, deps: readonly unknown[]) {
      const s = slot();
      if (!s.deps || s.deps.some((v, i) => !Object.is(v, deps[i]))) {
        s.deps = deps;
        effect();
      }
    },
    useCallback(callback: () => void, deps: readonly unknown[]) {
      const s = slot();
      if (
        !s.deps ||
        s.deps.length !== deps.length ||
        s.deps.some((v, i) => !Object.is(v, deps[i]))
      ) {
        s.deps = deps;
        s.value = callback;
      }
      return s.value;
    },
  };
  const ports: Record<string, unknown> = {
    react,
    'react/jsx-runtime': {
      jsx: (type: unknown, props: any) => ({ type, props }),
      jsxs: (type: unknown, props: any) => ({ type, props }),
    },
    'expo-router': {
      useFocusEffect: (callback: typeof focus) => {
        focus = callback;
      },
      Stack: { Screen: 'Screen' },
      router: {},
    },
    'react-native': { View: 'View', Text: 'Text', ScrollView: 'ScrollView' },
    '../i18n/index.ts': { t: (key: string) => key },
    '../core/provider': { useClient: () => ({ store, ...state }) },
    '../core/query': {},
    '../components/dialogs': {},
    '../miniapps/use-device-consent': {},
    '../miniapps/device-consent': {},
    '../components/primitives': { Surface: 'Surface', NativeButton: 'NativeButton' },
    '../components/composer-action': { ComposerAction: 'ComposerAction' },
    '../theme': { useTheme: () => ({ colors: {} }) },
    '../miniapps/network-consent': {
      createNetworkConsent: () => {
        const scope = { closed: false, done: deferred<void>() };
        scopes.push(scope);
        return {
          read: async () => ({
            itemTitle: 'Maps',
            spaceTitle: 'Chat',
            manifest: {
              name: 'Maps',
              networkPurpose: 'Tiles',
              allowedDomains: ['a.example.org', 'b.example.org'],
            },
            allowedHosts: [],
          }),
          check: () => {
            if (scope.closed) throw Error('closed');
          },
          grant: () => scope.done.promise,
          close: () => {
            scope.closed = true;
          },
        };
      },
    },
  };
  const module = { exports: {} as { ItemNetworkAccess: (props: any) => Element | null } };
  runInNewContext(consumer.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const props = {
    itemId: 'item-a',
    grantRevision: 'no grants',
    onChange: () => {},
    onContext: () => {},
    consent: { ask: () => {}, cancelAll: () => {} },
  };
  return {
    state,
    scopes,
    props,
    render() {
      cursor = 0;
      return module.exports.ItemNetworkAccess(props);
    },
    commit() {
      if (focus !== committed) {
        cleanup?.();
        committed = focus;
        cleanup = focus?.() ?? null;
      }
    },
  };
}
function button(tree: unknown): Element | undefined {
  if (Array.isArray(tree)) {
    for (const child of tree) {
      const found = button(child);
      if (found) return found;
    }
    return;
  }
  if (!tree || typeof tree !== 'object' || !('props' in tree)) return;
  const element = tree as Element;
  if (element.type === 'NativeButton' && element.props.label === 'miniapps.enableOnlineMaps') {
    return element;
  }
  return button(element.props.children);
}
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

test('consumer defers grant projection refresh until the multi-host operation settles; item and owner fences stay immediate', async () => {
  const f = routeFixture();
  f.render();
  f.commit();
  await tick();
  button(f.render())!.props.onPress();
  f.commit();
  f.props.grantRevision = 'first host persisted, second consent still pending';
  f.render();
  f.commit();
  assert.equal(f.scopes.length, 1, 'its own first grant must not retire a still-running operation');
  assert.equal(f.scopes[0]!.closed, false);
  f.scopes[0]!.done.resolve();
  await tick();
  f.render();
  f.commit();
  await tick();
  assert.equal(f.scopes.length, 2, 'latest projection applies after actual completion');
  for (const change of ['item', 'owner'] as const) {
    const g = routeFixture();
    g.render();
    g.commit();
    await tick();
    button(g.render())!.props.onPress();
    g.commit();
    if (change === 'item') g.props.itemId = 'item-b';
    else g.state.snapshot.account.device_id = 'device-b';
    assert.equal(g.render(), null, 'old policy disappears before the replacement focus effect');
    g.commit();
    assert.equal(g.scopes[0]!.closed, true);
    assert.equal(g.scopes.length, 2);
    g.scopes[0]!.done.resolve();
    await tick();
  }
});
