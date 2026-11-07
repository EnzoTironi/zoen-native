import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import type { useAgentControl } from '../agents/use-agent-control';
import type { AgentControlDto } from './generated/AgentControlDto';
import { deferred } from './store.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../agents/use-agent-control.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  external: ['react', 'react/compiler-runtime', 'react-native', 'expo-router', '../core/provider'],
  plugins: [
    {
      name: 'expo-compiler',
      setup(build) {
        build.onLoad({ filter: /use-agent-control\.ts$/ }, ({ path }) => ({
          contents: compileExpoSource(path, true),
          loader: 'js',
          resolveDir: dirname(path),
        }));
      },
    },
  ],
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

async function fixture() {
  const root = createReactTestRoot(),
    reads: ReturnType<typeof deferred<AgentControlDto>>[] = [];
  const timers = new Map<number, () => void>(),
    owners = new Set<() => void>(),
    canonical = new Set<() => void>();
  const appListeners = new Set<(value: string) => void>();
  let sequence = 0,
    cursor = 0,
    writes = 0,
    enabled = true;
  let observed: ReturnType<typeof useAgentControl> | undefined;
  const snapshot = {
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
    getSnapshot: () => snapshot,
    subscribe: (callback: () => void) => {
      owners.add(callback);
      return () => owners.delete(callback);
    },
    subscribeCanonicalChanges: (callback: () => void) => {
      canonical.add(callback);
      return () => canonical.delete(callback);
    },
    registerSessionCleanup: () => () => {},
    driver: {
      request: (command: { op: string }) => {
        if (command.op === 'agent_profile')
          return Promise.resolve({
            persona: {
              id: 'agent',
              kind: 'Agent',
              is_mine: true,
              owner_id: 'owner',
            },
          });
        assert.equal(command.op, 'agent_control_read');
        const read = deferred<AgentControlDto>();
        reads.push(read);
        return read.promise;
      },
    },
    request: async () => {
      writes++;
    },
  };
  const app = {
    currentState: 'active',
    addEventListener: (_: string, callback: (value: string) => void) => {
      appListeners.add(callback);
      return { remove: () => appListeners.delete(callback) };
    },
  };
  const module = { exports: {} as { useAgentControl: typeof useAgentControl } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    setTimeout: (callback: () => void) => {
      timers.set(++sequence, callback);
      return sequence;
    },
    clearTimeout: (id: number | undefined) => {
      if (id !== undefined) timers.delete(id);
    },
    require: (name: string) => {
      if (name === 'react') return React;
      if (name === 'react/compiler-runtime') return require(name);
      if (name === 'react-native') return { AppState: app };
      if (name === 'expo-router')
        return {
          useFocusEffect: (effect: React.EffectCallback) => React.useEffect(effect, [effect]),
        };
      if (name === '../core/provider') return { useClient: () => ({ store, ...snapshot }) };
      throw Error(name);
    },
  });
  function Probe() {
    observed = module.exports.useAgentControl('agent', enabled);
    return null;
  }
  const render = () => root.render(React.createElement(Probe));
  const current = () => {
    assert.ok(observed);
    return observed;
  };
  await render();
  return {
    reads,
    timers,
    appListeners,
    canonical,
    current,
    act: root.act,
    writes: () => writes,
    async deliver(value: AgentControlDto) {
      assert.ok(reads[cursor]);
      await root.act(() => reads[cursor++]!.resolve(value));
    },
    async fail() {
      assert.ok(reads[cursor]);
      await root.act(() => reads[cursor++]!.reject(Error('offline')));
    },
    async tick() {
      const next = timers.entries().next();
      assert.ok(!next.done);
      timers.delete(next.value[0]);
      await root.act(next.value[1]);
    },
    async app(value: string) {
      app.currentState = value;
      await root.act(() => {
        for (const callback of appListeners) callback(value);
      });
    },
    async reopen() {
      store.sessionEpoch++;
      await root.act(() => {
        for (const callback of owners) callback();
      });
      await render();
    },
    async enable(value: boolean) {
      enabled = value;
      await render();
    },
    async close() {
      await root.close();
      assert.equal(timers.size, 0);
      assert.deepEqual(root.errors, []);
    },
  };
}

test('compiled visible control observes remote pause/revoke without conversation events or repeated effects', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    assert.equal(f.timers.size, 1);
    await f.tick();
    await f.deliver(record('2', 'Paused'));
    assert.equal(f.current().state.status, 'Paused');
    assert.equal(f.current().session.ticket()?.action, 'Resume');
    await f.tick();
    await f.deliver(record('3', 'Revoked'));
    assert.equal(f.current().state.status, 'Revoked');
    assert.equal(f.current().session.ticket(), null);
    assert.equal(f.timers.size, 0);
    assert.equal(f.writes(), 0);
  } finally {
    await f.close();
  }
});

test('polling and canonical bursts share the existing single-flight status read', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    await f.tick();
    assert.equal(f.timers.size, 0);
    await f.act(() => {
      for (let i = 0; i < 100; i++) for (const callback of f.canonical) callback();
    });
    assert.equal(f.reads.length, 2);
    await f.deliver(record('2'));
    assert.equal(f.reads.length, 3);
    assert.equal(f.timers.size, 0);
    await f.deliver(record('3', 'Paused'));
    assert.equal(f.timers.size, 1);
    assert.equal(f.current().state.status, 'Paused');
    assert.equal(f.writes(), 0);
  } finally {
    await f.close();
  }
});

test('background and retired timer/app callbacks cannot borrow the next account activation', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    const retiredTimer = [...f.timers.values()][0]!,
      retiredApp = [...f.appListeners][0]!;
    await f.app('background');
    assert.equal(f.timers.size, 0);
    await f.act(retiredTimer);
    assert.equal(f.reads.length, 1);
    await f.app('active');
    await f.deliver(record('2'));
    await f.reopen();
    await f.deliver(record('3', 'Paused'));
    const current = f.current().state;
    await f.act(() => {
      retiredTimer();
      retiredApp('background');
      retiredApp('active');
    });
    assert.equal(f.current().state, current);
    assert.equal(f.reads.length, 3);
    assert.equal(f.writes(), 0);
  } finally {
    await f.close();
  }
});

test('unknown receipt reconciliation only reads, and errors stop background observation', async () => {
  const f = await fixture();
  try {
    await f.deliver({
      ...record('1'),
      pending: 'Pause',
      operation_id: '11'.repeat(32),
      outcome: 'Unknown',
    });
    assert.equal(f.current().session.retryTicket()?.action, 'Pause');
    await f.tick();
    await f.deliver(record('2', 'Paused'));
    assert.equal(f.current().state.pending, null);
    await f.tick();
    await f.fail();
    assert.equal(f.current().state.phase, 'failed');
    assert.equal(f.timers.size, 0);
    assert.equal(f.writes(), 0);
  } finally {
    await f.close();
  }
});

test('disabling the control removes its observer until a fresh authorized read completes', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    const retired = [...f.timers.values()][0]!;
    await f.enable(false);
    await f.act(retired);
    assert.equal(f.timers.size, 0);
    assert.equal(f.reads.length, 1);
    await f.enable(true);
    assert.equal(f.timers.size, 0);
    await f.deliver(record('2'));
    assert.equal(f.timers.size, 1);
    assert.equal(f.writes(), 0);
  } finally {
    await f.close();
  }
});
