import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import * as scope from './account-scope.ts';
import * as port from '../owner-policy/account-port.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import type { PolicyTarget } from '../owner-policy/session.ts';
import type { AccountBudgetScreen } from '../agents/account-budget-screen';
const require = createRequire(import.meta.url);
const code = compileExpoSource(new URL('../agents/account-budget-screen.tsx', import.meta.url));
type Row = { id: string; disabled?: boolean; onPress(): void };

export async function accountBudgetReactFixture(
  initiallyEmpty = false,
  agentId?: string,
  initialOpening: Promise<unknown> | null = null,
) {
  const root = createReactTestRoot();
  const driver = new Driver(),
    initial = draftSession();
  const owner = initial.conversations[0]!.members[0]!;
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
  let snapshot = {
    ...initial,
    requests: [],
    agents: initiallyEmpty ? [] : [profile],
    conversations: [
      {
        ...initial.conversations[0]!,
        id: 'private-budget',
        kind: 'Direct' as const,
        counterpart: agent,
        members: [owner, agent],
      },
    ],
  };
  const commands: string[] = [];
  let opening: Promise<unknown> | null = initialOpening;
  driver.opening = async () => snapshot;
  driver.operation = async (command) => {
    commands.push(command.op);
    if (command.op === 'snapshot') return snapshot;
    assert.equal(command.op, 'start_owner_policy_direct');
    return opening ?? 'private-budget';
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const ui: { rows: Row[]; target: PolicyTarget | null } = { rows: [], target: null };
  const listeners = new Set<(state: string) => void>();
  const app = {
    currentState: 'active',
    addEventListener: (_: string, callback: (state: string) => void) => {
      listeners.add(callback);
      return { remove: () => listeners.delete(callback) };
    },
  };
  const module = { exports: {} as { AccountBudgetScreen: typeof AccountBudgetScreen } };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react') return React;
      if (
        name === 'react/jsx-runtime' ||
        name === 'react/compiler-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'react-native')
        return {
          AppState: app,
          Text: () => null,
          ScrollView: ({ children }: { children: React.ReactNode }) => children,
        };
      if (name === 'expo-router')
        return {
          Stack: { Screen: () => null },
          useFocusEffect: (effect: React.EffectCallback) => React.useEffect(effect, [effect]),
        };
      if (name === '../core/provider')
        return {
          useClient: () => ({
            store,
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
          }),
        };
      if (name === '../core/account-scope') return scope;
      if (name === '../owner-policy/account-port') return port;
      if (name === './owner-policy-screen')
        return {
          OwnerPolicyControllerScreen: ({ target }: { target: PolicyTarget }) => {
            ui.rows = [];
            ui.target = target;
            return null;
          },
        };
      if (name === '../components/settings-rows')
        return {
          SettingsRows: ({ rows }: { rows: Row[] }) => {
            ui.rows = rows;
            ui.target = null;
            return null;
          },
        };
      if (name === '../components/primitives')
        return {
          EmptyState: () => {
            ui.rows = [];
            ui.target = null;
            return null;
          },
        };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === '../theme') return { useTheme: () => ({ colors: { danger: 'red' } }) };
      throw Error(name);
    },
  });
  await root.render(React.createElement(module.exports.AccountBudgetScreen, { agentId }));
  return {
    ui,
    selection: () => ui.target,
    commands,
    listeners,
    store,
    act: root.act,
    hold: (value: Promise<unknown>) => {
      opening = value;
    },
    async addAgent() {
      snapshot = { ...snapshot, agents: [profile] };
      await root.act(() => store.refresh());
    },
    async reopen() {
      await root.act(async () => {
        await store.close();
        await store.open('account');
      });
    },
    async app(state: string) {
      app.currentState = state;
      await root.act(() => {
        for (const callback of listeners) callback(state);
      });
    },
    async close() {
      await root.close();
      await store.close();
      assert.deepEqual(root.errors, []);
    },
  };
}
