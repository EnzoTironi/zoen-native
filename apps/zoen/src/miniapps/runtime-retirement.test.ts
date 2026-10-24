import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { installRuntimeClient } from './runtime-browser.ts';
import { parseRuntimePackage } from './runtime-package.ts';

function resource(module: string) {
  const revision = 'a'.repeat(40);
  const wasm = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
  const hash = (value: string | Uint8Array) => createHash('sha256').update(value).digest('hex');
  const runtime = parseRuntimePackage({
    schema: 1,
    engine: 'wordcraft',
    revision,
    abi: 'zoen-host-v1',
    module: {
      url: `https://code.example.test/${revision}/engine.js`,
      bytes: Buffer.byteLength(module),
      sha256: hash(module),
      mime: 'text/javascript',
    },
    wasm: {
      url: `https://code.example.test/${revision}/engine.wasm`,
      bytes: wasm.length,
      sha256: hash(wasm),
      mime: 'application/wasm',
    },
  });
  return { runtime, revision, wasm };
}

test('pagehide clears document callbacks before retiring its loaded engine', async () => {
  const create = URL.createObjectURL;
  const revoke = URL.revokeObjectURL;
  const shared = globalThis as typeof globalThis & {
    retirementFixture?: { retired: boolean; events: string[] };
  };
  const module = `
    export default async () => {};
    export function zoenHostStart() {};
    export function zoenHostOnChange(callback) {
      if (globalThis.retirementFixture.retired) throw Error('engine already retired');
      if (callback !== undefined) throw Error('unexpected callback');
      globalThis.retirementFixture.events.push('clear');
    }
    export function zoenHostRetire() {
      globalThis.retirementFixture.events.push('retire');
      globalThis.retirementFixture.retired = true;
    }
  `;
  const { runtime, revision, wasm } = resource(module);
  URL.createObjectURL = () =>
    `data:text/javascript;base64,${Buffer.from(module).toString('base64')}`;
  URL.revokeObjectURL = () => {};
  try {
    for (const failingCleanup of [false, true]) {
      const state = { retired: false, events: [] as string[] };
      shared.retirementFixture = state;
      const target = new EventTarget() as EventTarget & { parent: object };
      let receive: EventListener | undefined;
      const add = target.addEventListener.bind(target);
      target.addEventListener = (name, listener, options) => {
        if (name === 'message') receive = listener as EventListener;
        add(name, listener, options);
      };
      target.parent = { postMessage() {} };
      const client = installRuntimeClient(runtime, target as unknown as Window);
      const pending = client.load();
      receive?.({
        source: target.parent,
        data: { zoenRuntime: 1, kind: 'ready', revision, module, wasm },
      } as unknown as MessageEvent);
      const engine = await pending;
      let live = true;
      let cleanups = 0;
      const retireOwner = () => {
        if (!live) return;
        live = false;
        cleanups++;
        (engine.zoenHostOnChange as (value: undefined) => void)(undefined);
        client.dispose(); // Owner/runtime disposal is deliberately reentrant.
        if (failingCleanup) throw Error('owner cleanup failure');
      };
      client.onBeforeDispose(retireOwner);
      assert.throws(() => client.onBeforeDispose(() => {}), /already has an owner/);
      if (failingCleanup) {
        assert.throws(() => client.dispose(), /owner cleanup failure/);
      } else {
        // The runtime's pagehide listener was installed before the App's.
        target.addEventListener('pagehide', retireOwner, { once: true });
        target.dispatchEvent(new Event('pagehide'));
      }
      assert.deepEqual(state.events, ['clear', 'retire']);
      assert.equal(cleanups, 1);
      client.dispose();
      retireOwner();
      assert.equal(cleanups, 1);
      assert.deepEqual(state.events, ['clear', 'retire']);
      assert.throws(() => client.load(), /retired/);
      assert.throws(() => client.onBeforeDispose(() => {}), /retired/);
    }
  } finally {
    URL.createObjectURL = create;
    URL.revokeObjectURL = revoke;
    delete shared.retirementFixture;
  }
});

test('close during real WASM initialization retires only initialized bindings and never starts a late App', async () => {
  const create = URL.createObjectURL;
  const revoke = URL.revokeObjectURL;
  const shared = globalThis as typeof globalThis & {
    pendingRuntime?: {
      compiled(): void;
      continuation: Promise<void>;
      fail: boolean;
      initialized: boolean;
      starts: number;
      retires: number;
    };
  };
  const module = `
    export default async ({module_or_path}) => {
      await WebAssembly.instantiate(module_or_path);
      const state = globalThis.pendingRuntime;
      state.compiled();
      await state.continuation;
      if (state.fail) throw Error('initialization failed');
      state.initialized = true;
    };
    export function zoenHostStart() { globalThis.pendingRuntime.starts++; };
    export function zoenHostRetire() {
      const state = globalThis.pendingRuntime;
      if (!state.initialized) throw Error('exports are not initialized');
      state.retires++;
    }
  `;
  const { runtime, revision, wasm } = resource(module);
  URL.createObjectURL = () =>
    `data:text/javascript;base64,${Buffer.from(module).toString('base64')}`;
  URL.revokeObjectURL = () => {};
  try {
    for (const fail of [false, true]) {
      let release!: () => void;
      let compiled!: () => void;
      const entered = new Promise<void>((resolve) => {
        compiled = resolve;
      });
      const state = {
        compiled: () => compiled(),
        continuation: new Promise<void>((resolve) => {
          release = resolve;
        }),
        fail,
        initialized: false,
        starts: 0,
        retires: 0,
      };
      shared.pendingRuntime = state;
      const events = new Map<string, EventListener>();
      const parent = { postMessage() {} };
      const target = {
        parent,
        addEventListener: (name: string, listener: EventListener) => events.set(name, listener),
        removeEventListener: (name: string) => events.delete(name),
      } as unknown as Window;
      const client = installRuntimeClient(runtime, target);
      const pending = client.load();
      const rejected = assert.rejects(pending, /retired/);
      const receiving = events.get('message')?.({
        source: parent,
        data: { zoenRuntime: 1, kind: 'ready', revision, module, wasm },
      } as unknown as MessageEvent);
      await entered;
      assert.doesNotThrow(() => client.dispose());
      client.dispose();
      assert.equal(state.retires, 0, 'imported bindings do not yet have callable exports');
      release();
      await receiving;
      await rejected;
      assert.equal(state.starts, 0);
      assert.equal(state.retires, fail ? 0 : 1);
      client.dispose();
      assert.equal(state.retires, fail ? 0 : 1);
      assert.equal(events.size, 0);
    }
  } finally {
    URL.createObjectURL = create;
    URL.revokeObjectURL = revoke;
    delete shared.pendingRuntime;
  }
});
