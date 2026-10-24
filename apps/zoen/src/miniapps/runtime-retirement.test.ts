import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { installRuntimeClient } from './runtime-browser.ts';
import { parseRuntimePackage } from './runtime-package.ts';

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
