import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import type { useManagedAgent } from '../managed-agent/use-managed-agent';
import type { ManagedEnrollmentDto } from './generated/ManagedEnrollmentDto';
import type { ManagedRecoveryPageDto } from './generated/ManagedRecoveryPageDto';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../managed-agent/use-managed-agent.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  external: ['react', 'react-native', 'expo-router', '../core/provider'],
  define: { 'process.env.EXPO_OS': '"ios"' },
});

export function managedView(state = 'review'): ManagedEnrollmentDto {
  return {
    operation_id: 'original',
    state,
    name: 'Assistant',
    handle: 'assistant',
    agent_id: 'agent',
    preview:
      state === 'review'
        ? {
            agent_id: 'agent',
            offer_digest: 'reviewed-digest',
            name: 'Assistant',
            handle: 'assistant',
            persona: 'Help me',
            rules: ['Ask before effects'],
            expires_ms: String(Date.now() + 60_000),
            pairing_check: '',
          }
        : null,
  };
}

/** Actual hook, session and Core port; only native requests, navigation and time are synthetic. */
export async function managedAgentHookFixture(initial: ManagedEnrollmentDto | null = null) {
  const root = createReactTestRoot();
  const owners = new Set<() => void>();
  const preflights = new Set<() => void>();
  const appListeners = new Set<(next: string) => void>();
  const timers = new Map<number, { callback(): void; delay: number }>();
  const commands: { op: string; [key: string]: unknown }[] = [];
  let sequence = 0;
  let status: () => Promise<ManagedEnrollmentDto | null> = async () => initial;
  let page: ManagedRecoveryPageDto = { requests: [], next_operation: null };
  let observed: ReturnType<typeof useManagedAgent> | undefined;
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        registered: true,
        unlocked: true,
        setup_pending: false,
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: 'relay',
      },
    },
  };
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    subscribe: (callback: () => void) => {
      owners.add(callback);
      return () => owners.delete(callback);
    },
    registerSessionPreflight: (callback: () => void) => {
      preflights.add(callback);
      return () => preflights.delete(callback);
    },
    driver: {
      request: async (command: { op: string }) => {
        commands.push(command);
        if (command.op === 'managed_agent_status') return status();
        if (command.op === 'managed_agent_discover') return page;
        if (command.op === 'managed_agent_approve') return managedView('installing');
        throw Error(`Unexpected command ${command.op}`);
      },
    },
  };
  const app = {
    currentState: 'active',
    addEventListener: (_: string, callback: (next: string) => void) => {
      appListeners.add(callback);
      return { remove: () => appListeners.delete(callback) };
    },
  };
  const module = { exports: {} as { useManagedAgent: typeof useManagedAgent } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    setTimeout: (callback: () => void, delay: number) => {
      timers.set(++sequence, { callback, delay });
      return sequence;
    },
    clearTimeout: (id: number | undefined) => {
      if (id !== undefined) timers.delete(id);
    },
    require: (name: string) => {
      if (name === 'react') return React;
      if (name === 'react-native') return { AppState: app };
      if (name === '../core/provider') return { useClient: () => ({ store, ...state }) };
      if (name === 'expo-router')
        return {
          useFocusEffect: (effect: React.EffectCallback) => React.useEffect(effect, [effect]),
          router: {
            push: () => assert.fail('No navigation expected'),
            dismissAll: () => assert.fail('No navigation expected'),
          },
        };
      throw Error(name);
    },
  });
  function Probe() {
    observed = module.exports.useManagedAgent();
    return null;
  }
  const render = () => root.render(React.createElement(Probe));
  const current = () => {
    assert.ok(observed);
    return observed;
  };
  await render();
  return {
    commands,
    timers,
    appListeners,
    preflights,
    current,
    render,
    status: (read: typeof status) => {
      status = read;
    },
    page: (next: ManagedRecoveryPageDto) => {
      page = next;
    },
    act: root.act,
    async tick() {
      const next = timers.entries().next();
      assert.ok(!next.done, 'Expected a scheduled observation');
      timers.delete(next.value[0]);
      await root.act(next.value[1].callback);
    },
    async app(next: string) {
      app.currentState = next;
      await root.act(() => {
        for (const callback of appListeners) callback(next);
      });
    },
    async reopen() {
      store.sessionEpoch++;
      await root.act(() => {
        for (const callback of owners) callback();
      });
      await render();
    },
    async close() {
      await root.close();
    },
  };
}
