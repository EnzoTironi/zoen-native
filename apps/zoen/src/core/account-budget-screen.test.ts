import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { ClientStore } from './store.ts';
import { Driver, deferred, draftSession } from './store.fixtures.ts';
import type { Snapshot } from './types.ts';
import * as scope from './account-scope.ts';
import * as port from '../owner-policy/account-port.ts';
const settle = async () => {
  for (let i = 0; i < 8; i++) await new Promise<void>((done) => setImmediate(done));
};
const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../agents/account-budget-screen.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: true,
  external: ['*'],
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
});
async function fixture() {
  const initial = draftSession(),
    owner = initial.conversations[0]!.members[0]!;
  const agent = {
    ...owner,
    id: 'owned-agent',
    name: 'My agent',
    kind: 'Agent' as const,
    owner_id: owner.id,
    is_mine: true,
    is_me: false,
  };
  const profile = {
    persona: agent,
    budget_limit_cents: null,
    budget_spent_cents: null,
    near_limit: false,
    spaces: [],
    pending_requests: 0,
    approvals_streak: 0,
    runs_on: '',
    owner_notes_pending: 0,
  };
  let snapshot: Snapshot = {
    ...initial,
    requests: [],
    agents: [
      profile,
      { ...profile, persona: { ...agent, id: 'foreign-agent', owner_id: 'other', is_mine: false } },
    ],
    conversations: [
      {
        ...initial.conversations[0]!,
        id: 'private-budget',
        kind: 'Direct',
        counterpart: agent,
        members: [owner, agent],
      },
    ],
  };
  const driver = new Driver();
  let startCalls = 0;
  let opening: Promise<unknown> | null = null;
  driver.opening = async () => snapshot;
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return snapshot;
    assert.equal(command.op, 'start_owner_policy_direct');
    assert.equal(command.agent_id, agent.id);
    assert.equal(command.expected_identity_id, snapshot.account!.identity_id);
    assert.equal(command.expected_device_id, snapshot.account!.device_id);
    startCalls++;
    return opening ?? 'private-budget';
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const slots: any[] = [];
  let cursor = 0,
    effect: (() => void | (() => void)) | null = null,
    cleanup: (() => void) | null = null;
  const native = {
    currentState: 'active',
    listeners: new Set<(value: string) => void>(),
    addEventListener(_name: string, fn: (value: string) => void) {
      native.listeners.add(fn);
      return { remove: () => native.listeners.delete(fn) };
    },
  };
  const react = {
    useState(initial: any) {
      const i = cursor++;
      if (!(i in slots)) slots[i] = typeof initial === 'function' ? initial() : initial;
      return [
        slots[i],
        (value: any) => {
          slots[i] = value;
        },
      ];
    },
    useRef(initial: any) {
      const i = cursor++;
      if (!(i in slots)) slots[i] = { current: initial };
      return slots[i];
    },
    useCallback(fn: any, deps: any[]) {
      const i = cursor++;
      const prev = slots[i];
      if (!prev || deps.some((v, j) => v !== prev.deps[j])) slots[i] = { fn, deps };
      return slots[i].fn;
    },
  };
  const module = { exports: {} as any };
  const require = (name: string): unknown => {
    if (name === 'react') return react;
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: unknown, props: unknown) => ({ type, props }),
        jsxs: (type: unknown, props: unknown) => ({ type, props }),
      };
    if (name === 'react-native')
      return { AppState: native, ScrollView: 'ScrollView', Text: 'Text' };
    if (name === 'expo-router')
      return {
        Stack: { Screen: 'Stack.Screen' },
        useFocusEffect: (fn: any) => {
          if (effect !== fn) {
            cleanup?.();
            effect = fn;
            cleanup = fn() ?? null;
          }
        },
      };
    if (name === '../core/provider') return { useClient: () => ({ store }) };
    if (name === '../core/account-scope') return scope;
    if (name === '../owner-policy/account-port') return port;
    if (name === './owner-policy-screen')
      return { OwnerPolicyControllerScreen: 'OwnerPolicyControllerScreen' };
    if (name === '../components/settings-rows') return { SettingsRows: 'SettingsRows' };
    if (name === '../components/primitives') return { EmptyState: 'EmptyState' };
    if (name === '../i18n/index.ts') return { t: (key: string) => key };
    if (name === '../theme') return { useTheme: () => ({ colors: { danger: 'red' } }) };
    throw Error(`unexpected real selector dependency ${name}`);
  };
  runInNewContext(compiled.outputFiles[0]!.text, { module, require });
  function nodes(node: any): any[] {
    return node?.props
      ? [
          node,
          ...(Array.isArray(node.props.children)
            ? node.props.children.flatMap(nodes)
            : nodes(node.props.children)),
        ]
      : [];
  }
  const render = () => {
    cursor = 0;
    return nodes(module.exports.AccountBudgetScreen({}));
  };
  render();
  return {
    store,
    render,
    starts: () => startCalls,
    hold: (value: Promise<unknown>) => {
      opening = value;
    },
    background(value: string) {
      native.currentState = value;
      for (const fn of native.listeners) fn(value);
    },
    blur() {
      cleanup?.();
      cleanup = null;
    },
    focus() {
      cleanup = effect?.() ?? null;
    },
    async close() {
      cleanup?.();
      await store.close();
    },
  };
}
const row = (f: Awaited<ReturnType<typeof fixture>>) => {
  const rows = f.render().find((node) => node.type === 'SettingsRows')?.props.rows;
  assert.equal(rows.length, 1, 'foreign agents never become budget services');
  return rows[0];
};
test('real account selector opens one scoped private context with no Request or financial command', async () => {
  const f = await fixture();
  const action = row(f);
  action.onPress();
  action.onPress();
  await settle();
  assert.equal(f.starts(), 1);
  const selected = f.render().find((node) => node.type === 'OwnerPolicyControllerScreen');
  assert.ok(selected);
  assert.deepEqual(JSON.parse(JSON.stringify(selected.props.target)), {
    purpose: 'account',
    agentId: 'owned-agent',
    spaceId: 'private-budget',
  });
  await f.close();
});
test('real selector discards a native response after account preflight replacement', async () => {
  const f = await fixture(),
    gate = deferred<unknown>();
  f.hold(gate.promise);
  row(f).onPress();
  await f.store.open('demo');
  gate.resolve('private-budget');
  await settle();
  assert.equal(f.starts(), 1);
  assert.equal(
    f.render().some((node) => node.type === 'OwnerPolicyControllerScreen'),
    false,
  );
  await f.close();
});
test('background and refocus cannot reuse old selector handlers or adopt an old in-flight result', async () => {
  const f = await fixture(),
    gate = deferred<unknown>();
  f.hold(gate.promise);
  const old = row(f);
  old.onPress();
  f.background('background');
  f.background('active');
  const fresh = row(f);
  old.onPress();
  assert.equal(f.starts(), 1, 'old rendered focus lease cannot open another control');
  gate.resolve('private-budget');
  await settle();
  assert.equal(
    f.render().some((node) => node.type === 'OwnerPolicyControllerScreen'),
    false,
  );
  f.hold(Promise.resolve('private-budget'));
  fresh.onPress();
  await settle();
  assert.equal(f.starts(), 2);
  assert.ok(f.render().some((node) => node.type === 'OwnerPolicyControllerScreen'));
  await f.close();
});
test('blurred selector cannot dispatch even a reversible private-context command', async () => {
  const f = await fixture();
  const old = row(f);
  f.blur();
  old.onPress();
  assert.equal(f.starts(), 0);
  f.focus();
  old.onPress();
  assert.equal(f.starts(), 0);
  row(f).onPress();
  await settle();
  assert.equal(f.starts(), 1);
  await f.close();
});
