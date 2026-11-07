import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import type { useAgentControl } from '../agents/use-agent-control';
import type { AgentControlDto } from './generated/AgentControlDto';
import { deferred } from './store.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../agents/use-agent-control.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  external: ['react', 'react-native', 'expo-router', '../core/provider'],
});
const record = (
  revision: string,
  status: AgentControlDto['status'] = 'Active',
): AgentControlDto => ({
  revision,
  status,
  pending: null,
  operation_id: null,
  outcome: 'Observed',
});

test('actual agent hook coalesces canonical bursts and cannot reactivate from a retired app callback', async () => {
  const root = createReactTestRoot();
  const canonical = new Set<() => void>();
  const owners = new Set<() => void>();
  const cleanups = new Set<() => Promise<void>>();
  const appListeners = new Set<(state: string) => void>();
  const requests: ReturnType<typeof deferred<AgentControlDto>>[] = [];
  let writes = 0;
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
        device_id: 'device',
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
    registerSessionCleanup: (callback: () => Promise<void>) => {
      cleanups.add(callback);
      return () => cleanups.delete(callback);
    },
    subscribeCanonicalChanges: (callback: () => void) => {
      canonical.add(callback);
      return () => canonical.delete(callback);
    },
    driver: {
      request: (command: { op: string }) => {
        if (command.op === 'agent_profile')
          return Promise.resolve({
            persona: { id: 'agent', kind: 'Agent', is_mine: true, owner_id: 'owner' },
          });
        assert.equal(command.op, 'agent_control_read');
        const request = deferred<AgentControlDto>();
        requests.push(request);
        return request.promise;
      },
    },
    request: async () => {
      writes++;
    },
  };
  const app = {
    currentState: 'active',
    addEventListener: (_: string, callback: (state: string) => void) => {
      appListeners.add(callback);
      return { remove: () => appListeners.delete(callback) };
    },
  };
  const module = { exports: {} as { useAgentControl: typeof useAgentControl } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    setTimeout,
    clearTimeout,
    require: (name: string) => {
      if (name === 'react') return React;
      if (name === 'react-native') return { AppState: app };
      if (name === 'expo-router')
        return {
          useFocusEffect: (effect: React.EffectCallback) => React.useEffect(effect, [effect]),
        };
      if (name === '../core/provider') return { useClient: () => ({ store, ...state }) };
      throw Error(name);
    },
  });
  let observed: ReturnType<typeof useAgentControl> | undefined;
  function Probe() {
    observed = module.exports.useAgentControl('agent', true);
    return null;
  }
  const current = () => {
    assert.ok(observed);
    return observed;
  };
  const emit = (state: string) => {
    app.currentState = state;
    appListeners.forEach((callback) => callback(state));
  };
  try {
    await root.render(React.createElement(Probe));
    assert.equal(requests.length, 1);
    await root.act(() => requests[0]!.resolve(record('1')));
    const oldTicket = current().session.ticket()!;
    await root.act(() => {
      for (let i = 0; i < 100; i++) canonical.forEach((callback) => callback());
    });
    assert.equal(requests.length, 2);
    assert.equal(current().session.ticket(), null);
    await root.act(() => {
      for (let i = 0; i < 100; i++) canonical.forEach((callback) => callback());
      requests[1]!.resolve(record('2'));
    });
    assert.equal(requests.length, 3);
    assert.equal(current().state.phase, 'loading');
    await root.act(() => emit('background'));
    await root.act(() => emit('active'));
    assert.equal(requests.length, 4);
    await root.act(() => requests[2]!.resolve(record('3', 'Paused')));
    assert.equal(current().state.phase, 'loading');
    await root.act(() => requests[3]!.resolve(record('4', 'Paused')));
    assert.equal(current().session.ticket()!.revision, '4');
    await root.act(() => current().session.change(oldTicket));
    assert.equal(writes, 0);
    await root.act(() => emit('background'));
    const retained = [...appListeners];
    const retiredCanonical = [...canonical];
    const retiredOwners = [...owners];
    const retiredCleanups = [...cleanups];
    const session = current().session;
    await root.close();
    app.currentState = 'active';
    retained.forEach((callback) => callback('active'));
    retiredCanonical.forEach((callback) => callback());
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(requests.length, 4);
    assert.equal(session.getSnapshot().phase, 'inactive');
    assert.equal(session.ticket(), null);
    assert.deepEqual(root.errors, []);
    // The WeakMap reuses this controller in the next account activation. Old
    // subscription/cleanup callbacks must not retire that new owner either.
    store.sessionEpoch++;
    const replacement = createReactTestRoot();
    try {
      await replacement.render(React.createElement(Probe));
      assert.equal(requests.length, 5);
      await replacement.act(() => requests[4]!.resolve(record('5')));
      await replacement.act(async () => {
        retiredOwners.forEach((callback) => callback());
        retiredCanonical.forEach((callback) => callback());
        retained.forEach((callback) => callback('background'));
        await Promise.all(retiredCleanups.map((callback) => callback()));
      });
      assert.equal(requests.length, 5);
      assert.equal(current().state.phase, 'ready');
      assert.equal(current().session.ticket()!.revision, '5');
      assert.deepEqual(replacement.errors, []);
    } finally {
      await replacement.close();
    }
  } finally {
    await root.close();
  }
});
