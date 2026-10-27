import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as scope from './account-scope.ts';
import { deferred } from './store.fixtures.ts';

type Element = { type: unknown; props: Record<string, any> };
const compiled = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/history-transfer.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    external: ['*'],
  })
).outputFiles[0]!.text;
const pending = {
  initial_pending: true,
  receive_active: false,
  manifest_received: false,
  chunks_received: 0,
  chunks_total: 0,
};
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
async function settle() {
  for (let i = 0; i < 20; ++i) await Promise.resolve();
}
function fixture() {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: 'https://relay.test',
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  };
  const listeners = new Set<() => void>(),
    preflights = new Set<() => void>(),
    foreground = new Set<(value: string) => void>();
  const calls: { op: string; epoch: number; device: string }[] = [];
  let refreshes = 0;
  let rpc: (command: any) => Promise<any> = async () => ({ ...pending });
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    driver: {
      request: async (command: any) => {
        calls.push({
          op: command.op,
          epoch: store.sessionEpoch,
          device: state.snapshot.account.device_id,
        });
        return rpc(command);
      },
    },
    refresh: async () => {
      ++refreshes;
    },
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    registerSessionPreflight: (fn: () => void) => {
      preflights.add(fn);
      return () => {
        preflights.delete(fn);
      };
    },
  };
  const app = {
    currentState: 'active',
    addEventListener: (_: string, fn: (value: string) => void) => {
      foreground.add(fn);
      return {
        remove: () => {
          foreground.delete(fn);
        },
      };
    },
  };
  const slots: any[] = [];
  let cursor = 0;
  const memo = (factory: () => unknown, deps: unknown[]) => {
    const i = cursor++;
    if (!slots[i] || !deps.every((v, n) => Object.is(v, slots[i].deps[n])))
      slots[i] = { value: factory(), deps };
    return slots[i].value;
  };
  let focus: (() => void | (() => void)) | undefined,
    nextFocus: typeof focus,
    cleanup: (() => void) | undefined;
  const timers = new Map<number, () => void>();
  let timer = 0;
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(compiled, {
    module,
    Error,
    setTimeout: (fn: () => void) => {
      timers.set(++timer, fn);
      return timer;
    },
    clearTimeout: (id: number) => {
      timers.delete(id);
    },
    require(name: string) {
      if (name === 'react')
        return {
          useState: (initial: unknown) => {
            const i = cursor++;
            if (!(i in slots)) slots[i] = initial;
            return [
              slots[i],
              (v: unknown) => {
                slots[i] = typeof v === 'function' ? v(slots[i]) : v;
              },
            ];
          },
          useRef: (value: unknown) => memo(() => ({ current: value }), []),
          useCallback: (fn: () => unknown, deps: unknown[]) => memo(() => fn, deps),
        };
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: unknown, props: object) => ({ type, props }),
          jsxs: (type: unknown, props: object) => ({ type, props }),
        };
      if (name === 'expo-router')
        return {
          useFocusEffect: (fn: typeof focus) => {
            if (fn !== focus) nextFocus = fn;
          },
        };
      if (name === 'react-native') return { Text: 'Text', AppState: app };
      if (name === '../core/provider') return { useClient: () => ({ ...state, store }) };
      if (name === '../core/account-scope') return scope;
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === './primitives') return { NativeButton: 'Button', Surface: 'Surface' };
      throw new Error(`Unexpected module ${name}`);
    },
  });
  const render = () => {
    cursor = 0;
    const tree = module.exports.HistoryTransfer();
    if (nextFocus) {
      cleanup?.();
      focus = nextFocus;
      nextFocus = undefined;
      cleanup = focus?.() || undefined;
    }
    return tree;
  };
  return {
    state,
    store,
    calls,
    render,
    rpc: (next: typeof rpc) => {
      rpc = next;
    },
    refreshes: () => refreshes,
    async ready() {
      render();
      await settle();
      return render();
    },
    button() {
      const button = nodes(render()).find((n) => n.type === 'Button');
      assert.ok(button);
      return button;
    },
    blur() {
      cleanup?.();
      cleanup = undefined;
    },
    focus() {
      cleanup = focus?.() || undefined;
    },
    app(value: string) {
      app.currentState = value;
      for (const fn of foreground) fn(value);
    },
    notify() {
      for (const fn of listeners) fn();
    },
    preflight() {
      for (const fn of preflights) fn();
    },
    dispose() {
      cleanup?.();
    },
  };
}

test('retained history Retry cannot adopt a new focus lease', async () => {
  const f = fixture();
  await f.ready();
  const old = f.button().props.onPress;
  f.blur();
  f.focus();
  await settle();
  f.render();
  f.calls.length = 0;
  old();
  await settle();
  assert.equal(f.calls.length, 0);
  f.button().props.onPress();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 1);
  f.dispose();
});

for (const retired of ['background', 'preflight', 'device', 'relay', 'epoch'] as const) {
  test(`history Retry does not dispatch after ${retired} retires its account lease`, async () => {
    const f = fixture();
    await f.ready();
    const old = f.button().props.onPress;
    f.calls.length = 0;
    if (retired === 'background') f.app('background');
    else if (retired === 'preflight') f.preflight();
    else {
      if (retired === 'device') f.state.snapshot.account.device_id = 'replacement';
      if (retired === 'relay') f.state.snapshot.account.relay_url = 'https://replacement.test';
      if (retired === 'epoch') ++f.store.sessionEpoch;
      f.notify();
    }
    old();
    await settle();
    assert.equal(f.calls.length, 0);
    f.dispose();
  });
}

test('late status cannot start receiving or refresh a replacement same-identity session', async () => {
  const f = fixture();
  await f.ready();
  const status = deferred<typeof pending>();
  f.rpc(async (command) => (command.op === 'history_status' ? status.promise : undefined));
  f.button().props.onPress();
  await settle();
  ++f.store.sessionEpoch;
  f.notify();
  status.resolve({ ...pending });
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 0);
  assert.equal(f.refreshes(), 0);
  f.dispose();
});

test('duplicate taps coalesce before React commits busy state', async () => {
  const f = fixture();
  await f.ready();
  const status = deferred<typeof pending>();
  f.rpc(async (command) => (command.op === 'history_status' ? status.promise : undefined));
  const retry = f.button().props.onPress;
  f.calls.length = 0;
  retry();
  retry();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'history_status').length, 1);
  status.resolve({ ...pending });
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 1);
  f.dispose();
});

test('already admitted Core receive survives retirement without a second receive or replacement refresh', async () => {
  const f = fixture();
  await f.ready();
  const receive = deferred<void>();
  f.rpc(async (command) => (command.op === 'receive_history' ? receive.promise : { ...pending }));
  f.button().props.onPress();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 1);
  f.blur();
  ++f.store.sessionEpoch;
  f.focus();
  await settle();
  receive.resolve();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 1);
  assert.equal(f.refreshes(), 0);
  f.dispose();
});

test('read failure remains visible and fresh Retry uses authoritative completed status without receiving', async () => {
  const f = fixture();
  f.rpc(async () => {
    throw new Error('history status unavailable');
  });
  const failed = await f.ready();
  assert.ok(
    nodes(failed).some(
      (n) =>
        n.props.accessibilityRole === 'alert' && n.props.children === 'history status unavailable',
    ),
  );
  f.rpc(async () => ({ ...pending, initial_pending: false }));
  f.button().props.onPress();
  await settle();
  f.render();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 0);
  assert.equal(f.refreshes(), 1);
  assert.equal(f.render(), null);
  f.dispose();
});

test('background/foreground does not reactivate a queued old Retry', async () => {
  const f = fixture();
  await f.ready();
  const old = f.button().props.onPress;
  f.app('background');
  f.app('active');
  await settle();
  f.render();
  f.calls.length = 0;
  old();
  await settle();
  assert.equal(f.calls.length, 0);
  f.button().props.onPress();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'receive_history').length, 1);
  f.dispose();
});
