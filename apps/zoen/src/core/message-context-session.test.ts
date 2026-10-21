import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { ClientStore } from './store.ts';
import type { Command, CommandResult, CoreDriver, Snapshot } from './types.ts';
import type { MessageContextStageHandle } from '../components/message-context-ports';

const empty: Snapshot = {
  account: null,
  me: null,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  conversations: [],
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
};
class Driver implements CoreDriver {
  readonly calls: string[] = [];
  async open() {
    this.calls.push('open');
    return empty;
  }
  async close() {
    this.calls.push('close');
  }
  async createAccount() {
    return empty;
  }
  async request<C extends Command>(_command: C): Promise<CommandResult<C>> {
    throw Error('This lifecycle does not request a document');
  }
  subscribe() {
    return () => {};
  }
  async eraseDevice() {}
}
const compiled = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/message-context-session.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    plugins: [
      {
        name: 'session-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => (resolve = done));
  return { promise, resolve };
}
const tick = () => new Promise<void>((done) => setImmediate(done));
type Node = { type: unknown; key: string; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
function fixture(stageFor: (scope: string) => MessageContextStageHandle) {
  const driver = new Driver();
  const store = new ClientStore(driver);
  const slots: Slot[] = [];
  let cursor = 0;
  let dirty = false;
  let effects: (() => void)[] = [];
  const next = () => (slots[cursor++] ??= {});
  const sessionCallbacks: (() => Promise<void>)[] = [];
  const deviceCallbacks: (() => Promise<void>)[] = [];
  const registerSession = store.registerSessionCleanup.bind(store);
  store.registerSessionCleanup = (callback) => {
    sessionCallbacks.push(() => callback(false));
    return registerSession(callback);
  };
  const registerDevice = store.registerDeviceCleanup.bind(store);
  store.registerDeviceCleanup = (callback) => {
    deviceCallbacks.push(callback);
    return registerDevice(callback);
  };
  const module = { exports: {} as { MessageContextSession: (props: any) => Node } };
  const jsx = (type: unknown, props: Record<string, unknown>, key: string) => ({
    type,
    props,
    key,
  });
  const ports: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => (next().value ??= { current: value }),
      useState: (initial: unknown) => {
        const slot = next();
        if (!('value' in slot)) slot.value = initial;
        return [
          slot.value,
          (value: unknown) => {
            if (slot.value !== value) {
              slot.value = value;
              dirty = true;
            }
          },
        ];
      },
      useLayoutEffect: (effect: () => (() => void) | undefined, deps: readonly unknown[]) => {
        const slot = next();
        if (slot.deps?.length === deps.length && deps.every((value, i) => value === slot.deps![i]))
          return;
        effects.push(() => {
          slot.cleanup?.();
          slot.deps = deps;
          slot.cleanup = effect();
        });
      },
    },
    'react/jsx-runtime': { jsx },
    '../core/provider': { useClient: () => ({ store, ...store.getSnapshot() }) },
    './message-context-source': { MessageContextStage: 'Stage' },
  };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  return {
    driver,
    store,
    sessionCallbacks,
    deviceCallbacks,
    render() {
      let tree: Node;
      do {
        dirty = false;
        cursor = 0;
        effects = [];
        tree = module.exports.MessageContextSession({ children: 'same navigation tree' });
      } while (dirty);
      // React commits the exact child ref before its parent's layout effects.
      tree.props.ref.current = stageFor(tree.key);
      effects.forEach((effect) => effect());
      return tree;
    },
    unmount() {
      slots.forEach((slot) => slot.cleanup?.());
    },
  };
}

test('cold open never retires the inactive initial native tag before the first driver open', async () => {
  let initialCalls = 0;
  const f = fixture(() => ({
    retireContexts: async () => {
      initialCalls++;
      throw Error('Unable to find ZoenMessageContextStage view with tag22');
    },
    dismissContexts: async () => {},
  }));
  assert.equal(f.render().props.active, false);
  await f.store.open('demo');
  assert.equal(initialCalls, 0);
  assert.equal(f.store.getSnapshot().phase, 'ready');
  assert.equal(f.store.getSnapshot().notice, null);
  assert.deepEqual(f.driver.calls, ['close', 'open']);
  f.unmount();
  await f.store.close();
});

test('immediate ready close still waits for the exact native Stage before closing the driver', async () => {
  const drain = deferred();
  let retirements = 0;
  const f = fixture(() => ({
    retireContexts: () => {
      retirements++;
      return drain.promise;
    },
    dismissContexts: async () => {},
  }));
  f.render();
  await f.store.open('demo');
  assert.equal(f.render().props.active, true);
  f.driver.calls.length = 0;
  const close = f.store.close();
  assert.equal(retirements, 1, 'ready owner admission is stopped synchronously');
  assert.deepEqual(f.driver.calls, []);
  f.render();
  await tick();
  assert.deepEqual(f.driver.calls, [], 'opening/retirement cannot bypass the old Stage drain');
  drain.resolve();
  await close;
  assert.deepEqual(f.driver.calls, ['close']);
  f.unmount();
});

test('a ready native retirement failure remains fail-closed and retains its failure and epoch', async () => {
  const failure = new Error('Native Stage retirement rejected');
  const f = fixture(() => ({
    retireContexts: async () => {
      throw failure;
    },
    dismissContexts: async () => {},
  }));
  f.render();
  await f.store.open('demo');
  f.render();
  const epoch = f.store.sessionEpoch;
  f.driver.calls.length = 0;
  await assert.rejects(f.store.close(), (error) => error === failure);
  assert.deepEqual(f.driver.calls, []);
  assert.equal(f.store.sessionEpoch, epoch);
  assert.equal(f.store.getSnapshot().notice, failure.message);
  f.unmount();
});

test('late session and device cleanup closures keep their captured Stage after a new owner mounts', async () => {
  const retired: string[] = [];
  const stages = new Map<string, MessageContextStageHandle>();
  const f = fixture((scope) => {
    if (!stages.has(scope))
      stages.set(scope, {
        retireContexts: async () => {
          retired.push(scope);
        },
        dismissContexts: async () => {},
      });
    return stages.get(scope)!;
  });
  f.render();
  await f.store.open('demo');
  const first = f.render();
  const oldSession = f.sessionCallbacks.at(-1)!;
  const oldDevice = f.deviceCallbacks.at(-1)!;
  await f.store.open('demo');
  const second = f.render();
  assert.notEqual(second.key, first.key);
  retired.length = 0;
  await oldSession();
  await oldDevice();
  assert.deepEqual(retired, [first.key, first.key]);
  await f.sessionCallbacks.at(-1)!();
  assert.equal(retired.at(-1), second.key);
  f.unmount();
  await f.store.close();
});
