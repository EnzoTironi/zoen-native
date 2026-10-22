import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as sessionModule from '../agent-definition/session.ts';
import { createDefinitionPort } from '../agent-definition/core-port.ts';
import { enrollmentScope } from '../agent-enrollment/session.ts';
import { t } from '../i18n/index.ts';
import type { ClientState } from './store.ts';
import type { Command } from './types.ts';
import type { AgentDefinitionDto } from './generated/AgentDefinitionDto.ts';
import type { AgentDefinitionUpdateDto } from './generated/AgentDefinitionUpdateDto.ts';

async function compile(path: string, loader: 'ts' | 'tsx') {
  const output = await build({
    stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    external: ['*'],
  });
  return output.outputFiles[0]!.text;
}
const hookSource = await compile('../agent-definition/use-agent-definition.ts', 'ts');
const routeSource = await compile('../app/agent-definition/[id].tsx', 'tsx');
const baseline: AgentDefinitionDto = {
  agent_id: 'agent',
  owner_id: 'owner',
  device_id: 'phone',
  relay_url: 'https://relay.example',
  status: 'Active',
  version: 1,
  definition_sha256: 'base',
  persona: 'Before',
  rules: ['Ask.'],
};
const review = {
  current: baseline,
  proposed: { ...baseline, persona: 'After', version: 2, definition_sha256: 'next' },
  preview_digest: 'digest',
};
const receipt: AgentDefinitionUpdateDto = {
  receipt_id: 'receipt',
  review,
  outcome: 'Unknown',
  current: baseline,
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
async function settle() {
  for (let i = 0; i < 16; i++) await Promise.resolve();
}
function hookFixture(platform = 'ios') {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: baseline.relay_url,
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  } as unknown as ClientState;
  const calls: Command[] = [],
    listeners = new Set<() => void>(),
    cleanups = new Set<() => Promise<void>>();
  let answer: (command: Command) => Promise<unknown> = async (command) =>
    command.op === 'agent_definition_reconcile' ? null : baseline;
  const request = async (command: Command) => {
    calls.push(command);
    return answer(command);
  };
  const store = {
    getSnapshot: () => state,
    driver: { request },
    request,
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    registerSessionCleanup: (fn: () => Promise<void>) => {
      cleanups.add(fn);
      return () => {
        cleanups.delete(fn);
      };
    },
  };
  let cursor = 0,
    focus: (() => () => void) | undefined,
    pendingFocus: typeof focus,
    cleanup: (() => void) | undefined;
  const slots: { value: any; deps: unknown[] }[] = [];
  const memo = (factory: () => any, deps: unknown[]) => {
    const i = cursor++;
    if (!slots[i] || !deps.every((v, n) => Object.is(v, slots[i]!.deps[n])))
      slots[i] = { value: factory(), deps };
    return slots[i]!.value;
  };
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(hookSource, {
    module,
    process: { env: { EXPO_OS: platform } },
    require: (name: string) => {
      if (name === 'react')
        return {
          useMemo: memo,
          useCallback: (fn: () => unknown, deps: unknown[]) => memo(() => fn, deps),
          useSyncExternalStore: (_subscribe: unknown, get: () => unknown) => get(),
        };
      if (name === 'expo-router')
        return {
          useFocusEffect: (fn: typeof focus) => {
            if (focus !== fn) pendingFocus = fn;
          },
        };
      if (name === '../core/provider') return { useClient: () => ({ store, ...state }) };
      if (name === '../agent-enrollment/session') return { enrollmentScope };
      if (name === './core-port') return { createDefinitionPort };
      if (name === './session') return sessionModule;
      throw Error(`Unexpected hook import ${name}`);
    },
  });
  return {
    state,
    calls,
    cleanups,
    handle: (fn: typeof answer) => {
      answer = fn;
    },
    changed: () => {
      for (const listener of listeners) listener();
    },
    render: (agent = 'agent') => {
      cursor = 0;
      const value = module.exports.useAgentDefinition(agent);
      if (pendingFocus) {
        cleanup?.();
        focus = pendingFocus;
        pendingFocus = undefined;
        cleanup = focus!();
      }
      return value as {
        session: sessionModule.DefinitionSession;
        state: sessionModule.DefinitionState;
      };
    },
    blur: () => cleanup?.(),
  };
}
for (const platform of ['web', 'android']) {
  test(`${platform} hook never probes an unsupported owner command`, async () => {
    const f = hookFixture(platform);
    f.render();
    await settle();
    assert.equal(f.render().state.phase, 'unavailable');
    assert.deepEqual(f.calls, []);
  });
}
test('real hook restores a held receipt before read/edit and focus cleanup never dismisses', async () => {
  const f = hookFixture();
  f.handle(async () => receipt);
  f.render();
  await settle();
  const mounted = f.render();
  assert.equal(mounted.state.phase, 'held');
  assert.deepEqual(
    f.calls.map((v) => v.op),
    ['agent_definition_reconcile'],
  );
  const ticket = mounted.session.ticket()!;
  f.blur();
  assert.equal(mounted.session.acceptsSession(ticket), false);
  assert.equal(
    f.calls.some((v) => v.op === 'agent_definition_dismiss' || v.op === 'agent_definition_update'),
    false,
  );
});
for (const field of ['identity_id', 'device_id', 'relay_url'] as const) {
  test(`hook retires pending receipt/read on ${field} change before React re-renders`, async () => {
    const f = hookFixture(),
      pending = deferred<AgentDefinitionUpdateDto | null>();
    f.handle(() => pending.promise);
    const first = f.render();
    f.state.snapshot!.account![field] = 'other';
    f.changed();
    pending.resolve(receipt);
    await settle();
    assert.equal(first.session.getSnapshot().review, null);
    assert.equal(first.session.getSnapshot().scope, '');
  });
}
for (const stage of ['reconcile', 'read']) {
  test(`hook retires a ${stage} callback after a raw relay swap with the same HTTP base`, async () => {
    const f = hookFixture(),
      pending = deferred<AgentDefinitionUpdateDto | AgentDefinitionDto>();
    f.state.snapshot!.account!.relay_url = 'https://relay.example/';
    f.handle((command) =>
      stage === 'read' && command.op === 'agent_definition_reconcile'
        ? Promise.resolve(null)
        : pending.promise,
    );
    const first = f.render(),
      old = first.session.ticket()!;
    await settle();
    assert.equal(f.calls.at(-1)?.op, `agent_definition_${stage}`);
    f.state.snapshot!.account!.relay_url = 'wss://relay.example';
    f.changed();
    pending.resolve(stage === 'read' ? baseline : receipt);
    await settle();
    assert.equal(first.session.acceptsSession(old), false);
    assert.equal(first.session.getSnapshot().scope, '');
    assert.equal(first.session.getSnapshot().current, null);
    assert.equal(first.session.getSnapshot().receipt, null);
    assert.equal(
      f.calls.some((command) => command.op === 'agent_definition_update'),
      false,
    );
  });
}
test('hook erasure cleanup and route A→B→A use a fresh generation, retaining only canonical receipts', async () => {
  const f = hookFixture(),
    pending = deferred<AgentDefinitionUpdateDto | null>();
  f.handle(() => pending.promise);
  const first = f.render(),
    old = first.session.ticket()!;
  f.render('other-agent');
  f.render('agent');
  pending.resolve(receipt);
  await settle();
  assert.equal(first.session.acceptsSession(old), false);
  for (const cleanup of f.cleanups) await cleanup();
  assert.equal(first.session.getSnapshot().scope, '');
});

async function routeFixture() {
  const calls: string[] = [];
  let held: AgentDefinitionUpdateDto | null = null;
  const session = new sessionModule.DefinitionSession({
    read: async () => baseline,
    reconcile: async () => {
      calls.push('reconcile');
      return held;
    },
    preview: async (_scope, base, persona, rules) => ({
      ...review,
      current: base,
      proposed: { ...review.proposed, persona, rules },
    }),
    update: async () => {
      calls.push('update');
      held = receipt;
      return receipt;
    },
    dismiss: async () => {
      calls.push('dismiss');
      held = null;
      return true;
    },
  });
  const scope = { owner: 'owner', device: 'phone', relay: baseline.relay_url, agent: 'agent' };
  session.activate(scope);
  await session.load();
  let cursor = 0,
    leaving = false,
    protectedState = false,
    prevent: ((value: { repeat: () => void }) => void) | undefined;
  const refs: { current: boolean }[] = [],
    dialogs: ReturnType<typeof deferred<boolean>>[] = [];
  let backs = 0,
    repeats = 0;
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(routeSource, {
    module,
    require: (name: string) => {
      if (name === 'react')
        return {
          useState: () => [
            leaving,
            (value: boolean) => {
              leaving = value;
            },
          ],
          useRef: (value: boolean) => (refs[cursor++] ??= { current: value }),
        };
      if (name === 'expo-router')
        return {
          useLocalSearchParams: () => ({ id: 'agent' }),
          router: {
            back: () => {
              backs++;
            },
          },
          usePreventRemove: (enabled: boolean, callback: typeof prevent) => {
            protectedState = enabled;
            prevent = callback;
          },
        };
      if (name === '../../agent-definition/use-agent-definition')
        return { useAgentDefinition: () => ({ session, state: session.getSnapshot() }) };
      if (name === '../../agent-definition/definition-view')
        return { DefinitionView: 'DefinitionView' };
      if (name === '../../components/dialogs')
        return {
          useDialogs: () => ({
            confirm: () => {
              const value = deferred<boolean>();
              dialogs.push(value);
              return value.promise;
            },
          }),
        };
      if (name === '../../i18n') return { t };
      if (name === 'react/jsx-runtime')
        return { jsx: (type: unknown, props: object) => ({ type, props }) };
      throw Error(`Unexpected route import ${name}`);
    },
  });
  const render = () => {
    cursor = 0;
    return module.exports.default() as { props: Record<string, (...args: any[]) => any> };
  };
  return {
    session,
    calls,
    scope,
    dialogs,
    render,
    back: () => {
      assert.equal(protectedState, true);
      prevent!({
        repeat: () => {
          repeats++;
        },
      });
    },
    counts: () => ({ backs, repeats, protectedState }),
  };
}
test('actual route discard dialog cannot discard later edits or another activated session', async () => {
  const f = await routeFixture();
  f.session.setPersona('local edit');
  f.render();
  f.back();
  f.session.setPersona('newer local edit');
  f.dialogs[0]!.resolve(true);
  await settle();
  assert.equal(f.counts().repeats, 0);
  f.render();
  f.back();
  f.session.activate({ ...f.scope, device: 'other' });
  f.dialogs[1]!.resolve(true);
  await settle();
  assert.equal(f.counts().repeats, 0);
});
test('actual route permits approved unsent discard but Back during held/submission never dismisses or resends', async () => {
  const f = await routeFixture();
  f.session.setPersona('local edit');
  f.render();
  f.back();
  f.dialogs[0]!.resolve(true);
  await settle();
  assert.equal(f.counts().repeats, 1);
  f.session.activate(f.scope);
  await f.session.load();
  f.session.setPersona('After');
  await f.session.preview();
  const tree = f.render();
  tree.props.onApply!();
  tree.props.onApply!();
  await settle();
  f.render();
  assert.equal(f.counts().protectedState, false);
  assert.equal(f.calls.filter((v) => v === 'update').length, 1);
  f.render().props.onDone!();
  assert.equal(f.counts().backs, 1);
  assert.equal(f.calls.includes('dismiss'), false);
});
test('stale actual route callbacks cannot edit, dismiss, navigate or apply on a new Agent session', async () => {
  const f = await routeFixture();
  f.session.setPersona('After');
  await f.session.preview();
  const old = f.render();
  f.session.activate({ ...f.scope, agent: 'other' });
  old.props.onPersona!('stale');
  old.props.onAddRule!();
  old.props.onApply!();
  old.props.onDone!();
  old.props.onDismissAttempt!();
  await settle();
  assert.equal(f.session.getSnapshot().persona, '');
  assert.equal(f.counts().backs, 0);
  assert.equal(f.calls.includes('update'), false);
  assert.equal(f.calls.includes('dismiss'), false);
});
