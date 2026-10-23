import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { DeviceSession } from '../devices/session.ts';
import { deferred } from './store.fixtures.ts';
import type { DeviceDto } from './generated/DeviceDto.ts';

type Element = { type: any; props: Record<string, any> };
const sources = {
  route: '../app/devices.tsx',
  list: '../devices/device-list.tsx',
  details: '../devices/device-details.tsx',
  rows: '../components/settings-rows.tsx',
};
const compiled = new Map(
  await Promise.all(
    Object.entries(sources).map(async ([name, path]) => {
      const result = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-device-screen-ports',
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
const phone = 'a'.repeat(64),
  peer = 'b'.repeat(64),
  revoked = 'c'.repeat(64);
const owner = { identity: 'owner', device: phone, relay: 'https://relay.test', epoch: 1 };
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
const text = (value: any): string =>
  typeof value === 'string'
    ? value
    : Array.isArray(value)
      ? value.map(text).join(' ')
      : value?.props
        ? text(value.props.children)
        : '';
async function fixture() {
  let current: typeof owner | null = { ...owner };
  let devices: DeviceDto[] = [
    { device_id: phone, this_device: true, revoked: false },
    { device_id: peer, this_device: false, revoked: false },
    { device_id: revoked, this_device: false, revoked: true },
  ];
  const calls: string[] = [];
  const confirmation = deferred<boolean>();
  const session = new DeviceSession({
    scope: () => current,
    read: async () => devices,
    revoke: async (id) => {
      calls.push(id);
      devices = devices.map((d) => (d.device_id === id ? { ...d, revoked: true } : d));
    },
  });
  session.activate(current);
  await session.load(session.ticket());
  const modules = new Map<string, any>();
  let link: unknown = null;
  const p = {
    list: {
      background: '#f2f2f7',
      block: '#fff',
      text: '#111',
      secondary: '#666',
      accent: '#08f',
      disclosure: '#aaa',
    },
    destructive: '#c00',
  };
  const t = (key: string, args?: Record<string, string>) =>
    key === 'settings.deviceSuffix' ? `Device ${args?.suffix}` : key;
  function load(name: string): any {
    if (modules.has(name)) return modules.get(name);
    const exports = {};
    const module = { exports };
    runInNewContext(compiled.get(name)!, {
      exports,
      module,
      process: { env: { EXPO_OS: 'ios' } },
      require(path: string) {
        if (path === 'react/jsx-runtime')
          return {
            jsx: (type: any, props: any) => ({ type, props }),
            jsxs: (type: any, props: any) => ({ type, props }),
          };
        if (path === 'react')
          return {
            useEffect: (effect: () => void) => effect(),
            useState: () => [
              link,
              (value: unknown) => {
                link = value;
              },
            ],
          };
        if (path === '@expo/ui')
          return {
            BottomSheet: 'BottomSheet',
            RNHostView: 'RNHostView',
            Host: 'Host',
            Column: 'Column',
            ListItem: 'ListItem',
            Text: 'NativeText',
          };
        if (path.endsWith('/modifiers'))
          return new Proxy(
            {},
            {
              get:
                (_, key) =>
                (...args: unknown[]) => ({ name: key, args }),
            },
          );
        if (path === 'react-native') return { View: 'View', Text: 'Text' };
        if (path.endsWith('/i18n')) return { t };
        if (path.endsWith('/theme'))
          return { useTheme: () => ({ presentation: p, scheme: 'light' }) };
        if (path.endsWith('/use-devices'))
          return {
            useDevices: () => ({
              state: session.getSnapshot(),
              session,
              accountRequired: !current,
            }),
          };
        if (path.endsWith('/dialogs'))
          return { useDialogs: () => ({ confirm: () => confirmation.promise }) };
        if (path.endsWith('/device-list')) return load('list');
        if (path.endsWith('/device-details')) return load('details');
        if (path.endsWith('/settings-rows')) return load('rows');
        if (path.endsWith('/settings-screen')) return { SettingsScreen: 'SettingsScreen' };
        if (path.endsWith('/profile-section')) return { ProfileSection: 'ProfileSection' };
        if (path.endsWith('/authorize-device')) return { AuthorizeDevice: 'AuthorizeDevice' };
        if (path.endsWith('/history-transfer')) return { HistoryTransfer: 'HistoryTransfer' };
        if (path.endsWith('/primitives'))
          return {
            NativeButton: 'NativeButton',
            IconButton: 'IconButton',
            Icon: 'Icon',
            EmptyState: 'EmptyState',
            Loading: 'Loading',
          };
        throw new Error(`Unexpected ${path}`);
      },
    });
    modules.set(name, module.exports);
    return module.exports;
  }
  const screen = load('route').default;
  function render(value: any): any {
    if (Array.isArray(value)) return value.map(render);
    if (!value?.props) return value;
    if (typeof value.type === 'function') return render(value.type(value.props));
    return { ...value, props: { ...value.props, children: render(value.props.children) } };
  }
  const tree = () => render(screen());
  const target = (id: string) => {
    const node = nodes(tree()).find((entry) => entry.props.testID === id);
    assert.ok(node, id);
    return node;
  };
  return {
    session,
    confirmation,
    calls,
    tree,
    target,
    retire() {
      current = null;
      session.retire();
    },
    async reactivate() {
      current = { ...owner };
      session.activate(current);
      await session.load(session.ticket());
    },
  };
}

test('actual device screen uses native rows and groups without exposing every full fingerprint', async () => {
  const f = await fixture();
  const tree = f.tree();
  assert.ok(nodes(tree).some((node) => node.props.testID === 'device-current'));
  assert.ok(nodes(tree).some((node) => node.props.testID === 'device-others'));
  assert.ok(nodes(tree).some((node) => node.props.testID === 'device-revoked'));
  assert.ok(!text(tree).includes(peer));
  f.target(`device-${peer}`).props.onPress();
  assert.equal(f.target('device-details-sheet').props.isPresented, true);
  assert.ok(text(f.tree()).includes(peer.match(/.{1,4}/g)!.join(' ')));
  assert.equal(f.target('device-revoke').props.destructive, true);
});

test('actual revoke button confirms the exact selected row and updates the real projection', async () => {
  const f = await fixture();
  f.target(`device-${peer}`).props.onPress();
  f.target('device-revoke').props.onPress();
  assert.equal(f.target('device-revoke').props.disabled, true);
  f.confirmation.resolve(true);
  for (let i = 0; i < 16; ++i) await Promise.resolve();
  assert.deepEqual(f.calls, [peer]);
  assert.equal(
    f.session.getSnapshot().devices.find((row) => row.device_id === peer)?.revoked,
    true,
  );
  assert.ok(!nodes(f.tree()).some((node) => node.props.testID === 'device-revoke'));
});

test('current and revoked details have no revoke action; retired confirmation cannot mutate', async () => {
  const f = await fixture();
  for (const id of [phone, revoked]) {
    f.target(`device-${id}`).props.onPress();
    assert.ok(!nodes(f.tree()).some((node) => node.props.testID === 'device-revoke'));
  }
  f.target(`device-${peer}`).props.onPress();
  f.target('device-revoke').props.onPress();
  f.retire();
  f.confirmation.resolve(true);
  for (let i = 0; i < 12; ++i) await Promise.resolve();
  assert.deepEqual(f.calls, []);
  assert.equal(f.target('device-details-sheet').props.isPresented, false);
});

test('linking remains behind an explicit action in a native sheet with the real authorization port', async () => {
  const f = await fixture();
  assert.equal(f.target('authorize-device-sheet').props.isPresented, false);
  f.target('device-link').props.onPress();
  assert.equal(f.target('authorize-device-sheet').props.isPresented, true);
  assert.ok(nodes(f.tree()).some((node) => node.type === 'AuthorizeDevice'));
  assert.ok(nodes(f.tree()).some((node) => node.type === 'HistoryTransfer'));
  f.retire();
  assert.equal(f.target('authorize-device-sheet').props.isPresented, false);
  await f.reactivate();
  assert.equal(f.target('authorize-device-sheet').props.isPresented, false);
});
