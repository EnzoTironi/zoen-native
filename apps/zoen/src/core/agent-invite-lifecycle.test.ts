import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { AgentControlSession } from '../agents/agent-control-session.ts';
import type { AccountScope } from './account-scope.ts';
import type { AgentControlDto } from './generated/AgentControlDto.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const path = new URL('../app/agent/[id].tsx', import.meta.url);
const code = transformSync(readFileSync(path, 'utf8'), {
  filename: path.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
type Element = { type: unknown; props: Record<string, any> };
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
function action(tree: Element, label: string) {
  const found = nodes(tree).find((node) => node.props.label === `agents.${label}`);
  assert.ok(found, label);
  return found;
}
async function settle() {
  for (let n = 0; n < 20; n++) await Promise.resolve();
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function fixture() {
  let scope: AccountScope = { identity: 'owner', device: 'phone', relay: 'relay', epoch: 1 };
  let record: AgentControlDto = {
    status: 'Active',
    revision: '1',
    operation_id: null,
    pending: null,
    outcome: 'Observed',
  };
  let fail = false;
  const controls: unknown[] = [];
  const confirmations: unknown[] = [];
  let confirmation: boolean | Promise<boolean> = false;
  const session = new AgentControlSession(
    {
      scope: () => scope,
      read: async () => {
        if (fail) throw Error('offline');
        return { ...record };
      },
      control: async (agent, action, observedScope, revision) => {
        controls.push({ agent, action, scope: observedScope, revision });
        record = {
          ...record,
          status: 'Revoked',
          revision: String(BigInt(revision) + 1n),
          outcome: 'Confirmed',
        };
      },
      retry: async () => {},
    },
    'agent',
  );
  session.activate(scope);
  await session.refresh();
  const requests: unknown[] = [],
    shares: unknown[] = [];
  let invite: Promise<{ link: string }> | null = null;
  const store = {
    driver: {
      request: async (command: unknown) => {
        requests.push(command);
        return invite ? await invite : { link: 'https://example.test/synthetic-invite' };
      },
    },
    request: async (command: unknown) => {
      requests.push(command);
    },
    refresh: async () => {},
  };
  const agent = {
    persona: { id: 'agent', name: 'Synthetic agent', is_mine: true, bio: '' },
    spaces: [],
    owner_notes_pending: 0,
  };
  const slots: unknown[] = [],
    cache: unknown[] = [];
  let cursor = 0;
  const jsx = (type: unknown, props: object) => ({ type, props });
  const primitives = Object.fromEntries(
    ['Avatar', 'EmptyState', 'Loading', 'NativeButton', 'Surface'].map((key) => [key, key]),
  );
  const ports: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react/compiler-runtime': {
      c: (length: number) => {
        if (!cache.length)
          cache.push(...Array(length).fill(Symbol.for('react.memo_cache_sentinel')));
        assert.equal(cache.length, length);
        return cache;
      },
    },
    react: {
      useState: (initial: unknown) => {
        const index = cursor++;
        if (!(index in slots)) slots[index] = initial;
        return [
          slots[index],
          (next: unknown) => {
            slots[index] = next;
          },
        ];
      },
    },
    'expo-router': { router: { push: () => {} }, useLocalSearchParams: () => ({ id: 'agent' }) },
    'react-native': {
      Pressable: 'Pressable',
      Text: 'Text',
      View: 'View',
      Share: {
        share: async (value: unknown) => {
          shares.push(value);
        },
      },
    },
    'expo-clipboard': {
      setStringAsync: async () => {
        throw Error('iOS Share only');
      },
    },
    '../../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en' },
    '../../core/provider': { useClient: () => ({ store, mode: 'account' }) },
    '../../core/query': {
      useCoreQuery: (query: { op: string } | null) => ({
        data: query?.op === 'agent_profile' ? agent : [],
        retry: () => {},
      }),
    },
    '../../core/presentation': { money: () => '' },
    '../../agents/trust-confirmation': {
      agentTrustLevels: () => [],
      confirmAgentTrust: async () => false,
    },
    '../../components/primitives': primitives,
    '../../components/settings-rows': { SettingsRows: 'SettingsRows' },
    '../../components/navigation-scroll-view': { NavigationScrollView: 'NavigationScrollView' },
    '../../theme': { useTheme: () => ({ colors: {} }) },
    '../../components/dialogs': {
      useDialogs: () => ({
        confirm: async (options: unknown) => {
          confirmations.push(options);
          return await confirmation;
        },
      }),
    },
    '../../agents/use-agent-control': {
      useAgentControl: () => ({ session, state: session.getSnapshot() }),
    },
    '../../agents/agent-control-view': { AgentControlView: 'AgentControlView' },
  };
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(code, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name in ports) return ports[name];
      throw Error(`Unexpected Agent import: ${name}`);
    },
  });
  return {
    session,
    controls,
    confirmations,
    confirm: (value: boolean | Promise<boolean>) => {
      confirmation = value;
    },
    reactivate: async () => {
      session.suspend();
      session.activate(scope);
      await session.refresh();
    },
    requests,
    shares,
    render: (): Element => {
      cursor = 0;
      return module.exports.default();
    },
    observe: async (patch: Partial<AgentControlDto>) => {
      record = { ...record, ...patch };
      await session.refresh();
    },
    failRead: async () => {
      fail = true;
      await session.refresh();
    },
    retireOwner: () => {
      scope = { ...scope, epoch: 2 };
    },
    holdInvite: (pending: Promise<{ link: string }>) => {
      invite = pending;
    },
  };
}
for (const status of ['Active', 'Paused'] as const) {
  test(`compiled Agent keeps invitation management for ${status}`, async () => {
    const f = await fixture();
    await f.observe({ status });
    assert.equal(action(f.render(), 'shareAgent').props.disabled, false);
    action(f.render(), 'shareAgent').props.onPress();
    await settle();
    action(f.render(), 'disableInviteLink').props.onPress();
    await settle();
    assert.deepEqual(JSON.parse(JSON.stringify(f.requests)), [
      { op: 'create_agent_invite', agent_id: 'agent' },
      { op: 'revoke_agent_invite', agent_id: 'agent' },
    ]);
    assert.equal(f.shares.length, 1);
  });
}
test('canonical revocation removes invitations and retires already rendered callbacks', async () => {
  const f = await fixture(),
    prior = f.render();
  await f.observe({ status: 'Revoked', revision: '2' });
  action(prior, 'shareAgent').props.onPress();
  action(prior, 'disableInviteLink').props.onPress();
  await settle();
  assert.deepEqual(f.requests, []);
  const tree = f.render();
  for (const label of ['shareAgent', 'disableInviteLink'])
    assert.equal(
      nodes(tree).some((node) => node.props.label === `agents.${label}`),
      false,
    );
  assert.ok(nodes(tree).some((node) => node.type === 'AgentControlView'));
  assert.ok(nodes(tree).some((node) => node.props.children === 'agents.recentActivity'));
  assert.ok(nodes(tree).some((node) => node.props.children === 'Synthetic agent'));
});
for (const blocked of ['pending', 'screen-inactive', 'failed', 'owner'] as const) {
  test(`invitation callbacks cannot mutate during ${blocked}`, async () => {
    const f = await fixture(),
      prior = f.render();
    if (blocked === 'pending')
      await f.observe({ pending: 'Pause', outcome: 'Unknown', operation_id: 'a'.repeat(64) });
    else if (blocked === 'screen-inactive') f.session.suspend();
    else if (blocked === 'failed') await f.failRead();
    else f.retireOwner();
    action(prior, 'shareAgent').props.onPress();
    action(prior, 'disableInviteLink').props.onPress();
    await settle();
    assert.deepEqual(f.requests, []);
    assert.equal(action(f.render(), 'shareAgent').props.disabled, true);
    assert.ok(nodes(f.render()).some((node) => node.type === 'AgentControlView'));
  });
}
test('late invite response cannot present Share after canonical revocation', async () => {
  const f = await fixture(),
    pending = deferred<{ link: string }>();
  f.holdInvite(pending.promise);
  action(f.render(), 'shareAgent').props.onPress();
  await settle();
  await f.observe({ status: 'Revoked', revision: '2' });
  pending.resolve({ link: 'https://example.test/synthetic-invite' });
  await settle();
  assert.equal(f.requests.length, 1, 'already admitted request is not replayed or cancelled');
  assert.deepEqual(f.shares, []);
});

test('a retained invite callback cannot borrow a new focus activation ticket', async () => {
  const f = await fixture(),
    prior = f.render();
  await f.reactivate();
  assert.ok(f.session.ticket(), 'the new activation itself remains usable');
  action(prior, 'shareAgent').props.onPress();
  action(prior, 'disableInviteLink').props.onPress();
  await settle();
  assert.deepEqual(f.requests, []);
  action(f.render(), 'shareAgent').props.onPress();
  await settle();
  assert.equal(f.requests.length, 1);
  assert.equal(f.shares.length, 1);
});

test('an admitted invite response cannot borrow a new activation to open Share', async () => {
  const f = await fixture(),
    pending = deferred<{ link: string }>();
  f.holdInvite(pending.promise);
  action(f.render(), 'shareAgent').props.onPress();
  await settle();
  await f.reactivate();
  assert.ok(f.session.ticket());
  pending.resolve({ link: 'https://example.test/synthetic-invite' });
  await settle();
  assert.equal(f.requests.length, 1, 'the admitted request is neither replayed nor cancelled');
  assert.deepEqual(f.shares, []);
});

for (const status of ['Active', 'Paused'] as const) {
  test(`compiled revoke confirms the exact ${status} canonical revision through the control session`, async () => {
    const f = await fixture();
    await f.observe({ status, revision: '7' });
    f.confirm(true);
    action(f.render(), 'revokeAgent').props.onPress();
    await settle();
    assert.equal(f.confirmations.length, 1);
    assert.deepEqual(JSON.parse(JSON.stringify(f.controls)), [
      {
        agent: 'agent',
        action: 'Revoke',
        scope: { identity: 'owner', device: 'phone', relay: 'relay', epoch: 1 },
        revision: '7',
      },
    ]);
    assert.deepEqual(
      f.requests,
      [],
      'the route must not bypass reviewed control with raw control_agent',
    );
    assert.equal(f.session.getSnapshot().status, 'Revoked');
  });
}

test('cancelling revoke leaves the canonical agent unchanged', async () => {
  const f = await fixture();
  f.confirm(false);
  action(f.render(), 'revokeAgent').props.onPress();
  await settle();
  assert.deepEqual(f.controls, []);
  assert.deepEqual(f.requests, []);
  assert.equal(f.session.getSnapshot().status, 'Active');
});

for (const changed of [
  'reactivation',
  'pause',
  'owner',
  'pending',
  'revoked',
  'observation',
] as const) {
  test(`revoke confirmation cannot outlive ${changed}`, async () => {
    const f = await fixture(),
      answer = deferred<boolean>();
    f.confirm(answer.promise);
    action(f.render(), 'revokeAgent').props.onPress();
    assert.equal(f.confirmations.length, 1);
    if (changed === 'reactivation') await f.reactivate();
    else if (changed === 'owner') f.retireOwner();
    else if (changed === 'pending')
      await f.observe({ pending: 'Pause', outcome: 'Unknown', operation_id: 'b'.repeat(64) });
    else if (changed === 'observation') await f.observe({});
    else await f.observe({ status: changed === 'pause' ? 'Paused' : 'Revoked', revision: '2' });
    answer.resolve(true);
    await settle();
    assert.deepEqual(f.requests, []);
    assert.deepEqual(f.controls, []);
  });
}

test('retained revoke press cannot open a confirmation in a new activation', async () => {
  const f = await fixture(),
    prior = f.render();
  await f.reactivate();
  action(prior, 'revokeAgent').props.onPress();
  assert.deepEqual(f.confirmations, []);
  assert.deepEqual(f.requests, []);
});
