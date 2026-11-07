import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import type { AgentControlView } from '../agents/agent-control-view';
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
const viewCode = compileExpoSource(new URL('../agents/agent-control-view.tsx', import.meta.url));
type Button = { testID: string; label: string; disabled?: boolean; onPress(): void };
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
  const buttons = new Map<string, Button>(),
    actions: string[] = [];
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
    request: async (command: { action?: string }) => {
      writes++;
      if (command.action) actions.push(command.action);
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
  const view = { exports: {} as { AgentControlView: typeof AgentControlView } };
  runInNewContext(viewCode, {
    module: view,
    exports: view.exports,
    require: (name: string) => {
      if (name === 'react') return React;
      if (
        name === 'react/compiler-runtime' ||
        name === 'react/jsx-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'react-native')
        return {
          Text: () => null,
          View: ({ children }: { children: React.ReactNode }) => children,
        };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../components/primitives')
        return {
          NativeButton: (props: Button) => {
            React.useLayoutEffect(() => {
              buttons.set(props.testID, props);
              return () => {
                buttons.delete(props.testID);
              };
            }, [props]);
            return null;
          },
        };
      throw Error(name);
    },
  });
  function Probe() {
    observed = module.exports.useAgentControl('agent', enabled);
    return React.createElement(view.exports.AgentControlView, {
      state: observed.state,
      session: observed.session,
    });
  }
  const render = () => root.render(React.createElement(Probe));
  const current = () => {
    assert.ok(observed);
    return observed;
  };
  await render();
  return {
    reads,
    buttons,
    actions,
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

test('the optimized rendered lifecycle button changes from Pause to Resume and dispatches the current reviewed action', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    const button = () => {
      const current = f.buttons.get('agent-lifecycle-change');
      assert.ok(current);
      return current;
    };
    assert.equal(button().label, 'agents.pauseAgent');
    assert.equal(button().disabled, false);
    await f.tick();
    await f.deliver(record('2', 'Paused'));
    assert.equal(button().label, 'agents.resumeAgent');
    assert.equal(button().disabled, false);
    await f.act(() => button().onPress());
    assert.deepEqual(f.actions, ['Resume']);
    await f.deliver(record('3'));
    assert.equal(button().label, 'agents.pauseAgent');
    assert.equal(button().disabled, false);
    assert.equal(f.writes(), 1);
  } finally {
    await f.close();
  }
});

test('an unchanged observation keeps rendered controls and a reviewed confirmation valid', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    const state = f.current().state,
      ticket = f.current().session.ticket(state);
    const button = f.buttons.get('agent-lifecycle-change');
    assert.ok(ticket);
    assert.ok(button);
    await f.tick();
    assert.equal(f.current().state, state);
    await f.deliver(record('1'));
    assert.equal(f.current().state, state);
    assert.equal(f.current().session.accepts(ticket), true);
    assert.equal(f.buttons.get('agent-lifecycle-change'), button);
    assert.equal(f.timers.size, 1);
  } finally {
    await f.close();
  }
});

test('an admitted control supersedes a passive read before adopting its fresh receipt', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    const button = f.buttons.get('agent-lifecycle-change');
    assert.ok(button);
    await f.tick();
    await f.act(() => button.onPress());
    assert.deepEqual(f.actions, ['Pause']);
    await f.deliver(record('1'));
    assert.equal(f.reads.length, 3);
    assert.equal(f.current().state.busy, true);
    await f.deliver(record('2', 'Paused'));
    assert.equal(f.current().state.status, 'Paused');
    assert.equal(f.current().state.pending, null);
    assert.equal(f.current().state.busy, false);
    assert.equal(f.writes(), 1);
  } finally {
    await f.close();
  }
});

test('an old passive completion cannot replace the next foreground observer', async () => {
  const f = await fixture();
  try {
    await f.deliver(record('1'));
    await f.tick();
    await f.app('background');
    await f.app('active');
    assert.equal(f.reads.length, 3);
    await f.act(() => f.reads[2]!.resolve(record('3', 'Paused')));
    const timer = [...f.timers.keys()];
    const current = f.current().state;
    await f.act(() => f.reads[1]!.resolve(record('2')));
    assert.deepEqual([...f.timers.keys()], timer);
    assert.equal(f.current().state, current);
    assert.equal(f.writes(), 0);
  } finally {
    await f.close();
  }
});
