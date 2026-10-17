import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import type { SurfacePort } from '../native-surfaces/coordinator.ts';

// Exercise the real iOS adapter against SDK-shaped native ports. A fresh install
// has no app-target donation registrations; the SDK rejects deleting such names.
const stubs: Record<string, string> = {
  expo: 'export const requireOptionalNativeModule = () => ({});',
  'expo-file-system':
    'export class File { exists = false; size = 0; } export const Paths = { document: "fixture" };',
  'expo-app-intents': `
    export const state = { queueClears: 0, catalogs: [], failKind: null };
    export const isAvailable = () => true;
    export async function clearPendingInvocationsAsync() { state.queueClears++; }
    export async function deleteDonationsAsync() { throw Error('UnregisteredDonationIntentException'); }
    export async function setEntityCatalogAsync(kind, records) {
      state.catalogs.push([kind, records]);
      if (state.failKind === kind) throw Error('Catalog unavailable');
    }
  `,
  './miniapp-widget.ios': 'export default { updateTimeline() {} };',
  './miniapp-activity.ios': 'export default { getInstances: () => [] };',
};
const output = await build({
  entryPoints: ['surface-test-entry'],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'native-surface-ports',
      setup(builder) {
        builder.onResolve({ filter: /^surface-test-entry$/ }, () => ({
          path: 'entry',
          namespace: 'entry',
        }));
        builder.onLoad({ filter: /.*/, namespace: 'entry' }, () => ({
          contents:
            'export { createSurfacePort } from "./platform.ios.ts"; export { state } from "expo-app-intents";',
          resolveDir: new URL('../native-surfaces', import.meta.url).pathname,
        }));
        builder.onResolve({ filter: /./ }, (args) =>
          args.path in stubs ? { path: args.path, namespace: 'native-fixture' } : undefined,
        );
        builder.onLoad({ filter: /.*/, namespace: 'native-fixture' }, (args) => ({
          contents: stubs[args.path],
        }));
      },
    },
  ],
});
const adapter = (await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
)) as {
  createSurfacePort(): Promise<SurfacePort>;
  state: { queueClears: number; catalogs: [string, unknown[]][]; failKind: string | null };
};

test('a fresh native install clears its actual intent queue without deleting unregistered donations', async () => {
  const port = await adapter.createSurfacePort();
  await port.eraseMetadata();
  assert.equal(adapter.state.queueClears, 1);
});

test('item catalog failure still attempts to remove the chat catalog and propagates failure', async () => {
  const port = await adapter.createSurfacePort();
  adapter.state.failKind = 'zoen.item';
  adapter.state.catalogs = [];
  await assert.rejects(port.catalogs([], []), /atalhos/);
  assert.deepEqual(adapter.state.catalogs, [
    ['zoen.item', []],
    ['zoen.chat', []],
  ]);
});
