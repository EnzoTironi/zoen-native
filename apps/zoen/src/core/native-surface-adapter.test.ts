import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import type { SurfacePort } from '../native-surfaces/coordinator.ts';

// Exercise the real iOS adapter against SDK-shaped native ports. A fresh install
// has no app-target donation registrations; the SDK rejects deleting such names.
const stubs: Record<string, string> = {
  expo: 'export const requireOptionalNativeModule = () => ({});',
  'expo-crypto': 'export const randomUUID = () => "activity_link_1";',
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
  './miniapp-activity.ios': `
    export const activityState = { instances: [] };
    export default {
      getInstances: () => activityState.instances,
      start(props, url, staleDate) {
        const instance = {
          props, url, staleDate, getId: () => 'activity_1',
          async update(props, staleDate) { this.props = props; this.staleDate = staleDate; },
          async end() { activityState.instances = activityState.instances.filter(value => value !== this); },
        };
        activityState.instances.push(instance);
        return instance;
      },
    };
  `,
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
            'import { initializeLocalization } from "../i18n/index.ts"; initializeLocalization("en"); export { createSurfacePort } from "./platform.ios.ts"; export { state } from "expo-app-intents"; export { activityState } from "./miniapp-activity.ios";',
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
  activityState: {
    instances: { props: { title: string; deepLink: string }; url: string; staleDate: Date }[];
  };
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
  await assert.rejects(port.catalogs([], []), /shortcuts/);
  assert.deepEqual(adapter.state.catalogs, [
    ['zoen.item', []],
    ['zoen.chat', []],
  ]);
});

test('activity starts bind the immutable native URL to a local session and retain normal updates', async () => {
  const port = await adapter.createSurfacePort();
  const owner = { identityId: 'id_a', deviceId: 'dev_a' };
  const content = {
    endsAt: Date.now() + 60_000,
    redacted: true,
    title: 'Zoen',
    bars: [],
    rows: [],
    accentHex: '#5B8DEF',
    symbol: 'timer',
    deepLink: 'zoen://surface/it_a?identityId=id_a&deviceId=dev_a&version=3',
  };
  const selection = port.startActivity(content, owner, 'it_a');
  assert.deepEqual(selection, { id: 'activity_1', linkId: 'activity_link_1' });
  const native = adapter.activityState.instances[0]!;
  const initialURL = native.url;
  assert.equal(
    initialURL,
    'zoen://activity/it_a?identityId=id_a&deviceId=dev_a&linkId=activity_link_1',
  );
  assert.equal(native.props.deepLink, initialURL);
  await port.updateActivity(selection.id, { ...content, title: 'Updated', deepLink: initialURL });
  assert.equal(native.url, initialURL);
  assert.equal(native.props.title, 'Updated');
  assert.equal(native.props.deepLink, initialURL);
  assert.equal(native.staleDate.getTime(), content.endsAt);
  await port.endActivity(selection.id);
  assert.deepEqual(port.activities(), []);
});
