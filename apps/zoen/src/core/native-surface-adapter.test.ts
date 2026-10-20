import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import type { SurfacePort } from '../native-surfaces/coordinator.ts';
import type { NativeWidgetCatalog } from '../native-surfaces/widget-catalog';
import type { WidgetTimelineCatalog } from '../native-surfaces/widget-publication';

// Exercise the real iOS adapter against SDK-shaped native ports. A fresh install
// has no app-target donation registrations; the SDK rejects deleting such names.
const stubs: Record<string, string> = {
  expo: 'export const requireOptionalNativeModule = () => ({});',
  'expo-crypto': 'export const randomUUID = () => "activity_link_1";',
  'expo-file-system': `
    export const fileState = { paths: [], reads: [], records: new Map() };
    export class File {
      constructor(...parts) { this.path = parts.join('/'); fileState.paths.push(this.path); }
      get exists() { return fileState.records.has(this.path); }
      get size() { return fileState.records.get(this.path)?.length ?? 0; }
      async text() { fileState.reads.push(this.path); return fileState.records.get(this.path); }
      write(value) { fileState.records.set(this.path, value); }
      delete() { fileState.records.delete(this.path); }
    }
    export const Paths = { document: 'fixture' };
  `,
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
  'expo-widgets': `
    export const widgetState = { timelines: [], options: [], failure: null };
    export function createWidget() {
      return {
        updateTimeline(entries) {
          widgetState.timelines.push(entries);
          if (widgetState.failure === 'timeline') throw Error('Widget timeline unavailable');
        },
        setConfigurationParameterEnum(parameter, values) {
          widgetState.options.push({ parameter, values });
          if (widgetState.failure === 'configuration') throw Error('Widget configuration unavailable');
        },
      };
    }
  `,
  '@expo/ui/swift-ui': `
    export const HStack='HStack', Image='Image', ProgressView='ProgressView', Text='Text', VStack='VStack';
  `,
  '@expo/ui/swift-ui/modifiers': `
    export const containerBackground=(...value)=>({kind:'containerBackground',value}),
      font=(...value)=>({kind:'font',value}), foregroundStyle=(...value)=>({kind:'foregroundStyle',value}),
      lineLimit=(...value)=>({kind:'lineLimit',value}), padding=(...value)=>({kind:'padding',value}),
      privacySensitive=(...value)=>({kind:'privacySensitive',value}), widgetURL=(...value)=>({kind:'widgetURL',value});
  `,
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
  jsx: 'automatic',
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
            'import { initializeLocalization } from "../i18n/index.ts"; initializeLocalization("en"); export { createSurfacePort } from "./platform.ios.ts"; export { MiniappWidget } from "./miniapp-widget.ios.tsx"; export { state } from "expo-app-intents"; export { activityState } from "./miniapp-activity.ios"; export { widgetState } from "expo-widgets"; export { fileState } from "expo-file-system";',
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
  widgetState: {
    timelines: { date: Date; props: WidgetTimelineCatalog }[][];
    options: { parameter: string; values: { name: string; value: string }[] }[];
    failure: 'timeline' | 'configuration' | null;
  };
  fileState: { paths: string[]; reads: string[]; records: Map<string, string> };
  MiniappWidget(catalog: WidgetTimelineCatalog, environment: Record<string, unknown>): WidgetTree;
};

type WidgetTree = {
  type: unknown;
  props: { children?: unknown; modifiers?: { kind: string; value: unknown[] }[] };
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

const selectionA = `w_${'a'.repeat(64)}`;
const selectionB = `w_${'b'.repeat(64)}`;
function configuredWidgets(): NativeWidgetCatalog {
  const placeholder = {
    redacted: true,
    title: 'Zoen',
    bars: [],
    rows: [],
    accentHex: '#5B8DEF',
    symbol: 'lock.fill',
  };
  return {
    placeholder,
    entries: [
      {
        selectionId: selectionA,
        configurationTitle: 'Private dinner',
        content: {
          ...placeholder,
          redacted: false,
          title: 'Private dinner',
          detail: 'Dinner at eight',
          targetMs: 1_900_000_000_000,
          deepLink: 'zoen://surface/it_a?identityId=id_a&deviceId=dev_a&version=3',
        },
      },
      {
        selectionId: selectionB,
        configurationTitle: 'Private flight',
        content: {
          ...placeholder,
          redacted: false,
          title: 'Private flight',
          detail: 'Flight tomorrow',
          deepLink: 'zoen://surface/it_b?identityId=id_a&deviceId=dev_a&version=7',
        },
      },
    ],
  };
}

function resetWidgetPort() {
  adapter.widgetState.timelines = [];
  adapter.widgetState.options = [];
  adapter.widgetState.failure = null;
}

function renderWidget(catalog: WidgetTimelineCatalog, selection?: string) {
  return adapter.MiniappWidget(catalog, {
    colorScheme: 'light',
    widgetFamily: 'systemSmall',
    configuration: selection === undefined ? undefined : { miniapp: selection },
  });
}

function widgetText(value: unknown): string[] {
  if (typeof value === 'string') return [value];
  if (Array.isArray(value)) return value.flatMap(widgetText);
  if (value && typeof value === 'object' && 'props' in value)
    return widgetText((value as WidgetTree).props.children);
  return [];
}

function widgetURL(value: WidgetTree) {
  return value.props.modifiers?.find((modifier) => modifier.kind === 'widgetURL')?.value[0];
}

test('the real native adapter publishes separate configuration options and expires each selected content with its own link', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 1_800_000_000_000 });
  resetWidgetPort();
  const port = await adapter.createSurfacePort();
  const catalog = configuredWidgets();
  const first = catalog.entries[0]!;
  const second = catalog.entries[1]!;
  const hidden = {
    ...catalog,
    entries: [
      first,
      {
        ...second,
        configurationTitle: 'Private widget 2',
        content: { ...catalog.placeholder, deepLink: second.content.deepLink },
      },
    ],
  };
  port.widget(hidden);
  assert.deepEqual(adapter.widgetState.options, [
    {
      parameter: 'miniapp',
      values: [
        { name: 'Choose an app', value: 'none' },
        { name: 'Private dinner', value: selectionA },
        { name: 'Private widget 2', value: selectionB },
      ],
    },
  ]);
  const timeline = adapter.widgetState.timelines[0]!;
  assert.equal(timeline.length, 2);
  assert.equal(timeline[0]?.date.getTime(), 1_800_000_000_000);
  assert.equal(timeline[1]?.date.getTime(), 1_800_000_900_000);
  assert.deepEqual(Object.keys(timeline[0]!.props.entries[0]!).sort(), ['content', 'selectionId']);
  assert.equal(JSON.stringify(timeline[0]?.props).includes('Private widget 2'), false);
  assert.equal(JSON.stringify(timeline).includes('Private flight'), false);
  assert.equal(timeline[0]?.props.entries[0]?.content.redacted, false);
  assert.equal(timeline[0]?.props.entries[1]?.content.redacted, true);
  for (const [index, expired] of timeline[1]!.props.entries.entries()) {
    assert.equal(expired.selectionId, hidden.entries[index]?.selectionId);
    assert.equal(expired.content.redacted, true);
    assert.equal(expired.content.title, 'Zoen');
    assert.equal(expired.content.targetMs, undefined);
    assert.deepEqual(expired.content.bars, []);
    assert.equal(expired.content.deepLink, hidden.entries[index]?.content.deepLink);
  }
  assert.equal(JSON.stringify(timeline[1]?.props).includes('Private dinner'), false);
});

test('the real widget layout selects A and B independently and never substitutes a first or remaining entry', () => {
  const catalog = configuredWidgets();
  const a = renderWidget(catalog, selectionA);
  const b = renderWidget(catalog, selectionB);
  assert.ok(widgetText(a).includes('Private dinner'));
  assert.equal(widgetText(a).includes('Private flight'), false);
  assert.ok(widgetText(b).includes('Private flight'));
  assert.equal(widgetText(b).includes('Private dinner'), false);
  assert.equal(widgetURL(a), catalog.entries[0]?.content.deepLink);
  assert.equal(widgetURL(b), catalog.entries[1]?.content.deepLink);
  for (const unknown of [undefined, 'none', 'missing', `w_${'c'.repeat(64)}`]) {
    const placeholder = renderWidget(catalog, unknown);
    assert.ok(widgetText(placeholder).includes('Zoen'));
    assert.equal(
      widgetText(placeholder).some((text) => text.includes('Private')),
      false,
    );
    assert.equal(widgetURL(placeholder), undefined);
  }
  const removed = { ...catalog, entries: [catalog.entries[1]!] };
  assert.ok(widgetText(renderWidget(removed, selectionA)).includes('Zoen'));
  assert.ok(widgetText(renderWidget(removed, selectionB)).includes('Private flight'));
  const anotherOwner = {
    ...catalog,
    entries: catalog.entries.map((entry, index) => ({
      ...entry,
      selectionId: `w_${String(index + 3).repeat(64)}`,
    })),
  };
  assert.ok(widgetText(renderWidget(anotherOwner, selectionA)).includes('Zoen'));
  assert.equal(widgetURL(renderWidget(anotherOwner, selectionA)), undefined);
});

test('native publication replaces removed choices and clears both timeline and configuration stores', async () => {
  resetWidgetPort();
  const port = await adapter.createSurfacePort();
  const catalog = configuredWidgets();
  port.widget(catalog);
  port.widget({ ...catalog, entries: [catalog.entries[1]!] });
  assert.deepEqual(
    adapter.widgetState.options.at(-1)?.values.map((option) => option.value),
    ['none', selectionB],
  );
  assert.deepEqual(
    adapter.widgetState.timelines.at(-1)?.[0]?.props.entries.map((entry) => entry.selectionId),
    [selectionB],
  );
  port.widget(null);
  assert.deepEqual(
    adapter.widgetState.options.at(-1)?.values.map((option) => option.value),
    ['none'],
  );
  const cleared = adapter.widgetState.timelines.at(-1)!;
  assert.equal(cleared.length, 1);
  assert.deepEqual(cleared[0]?.props.entries, []);
  assert.equal(cleared[0]?.props.placeholder.deepLink, undefined);
});

test('a failure in either native widget store attempts both independent redacted cleanup paths and propagates', async () => {
  for (const failure of ['timeline', 'configuration'] as const) {
    resetWidgetPort();
    const port = await adapter.createSurfacePort();
    adapter.widgetState.failure = failure;
    assert.throws(() => port.widget(configuredWidgets()), /Widget .* unavailable/);
    assert.deepEqual(adapter.widgetState.timelines.at(-1)?.[0]?.props.entries, []);
    assert.deepEqual(
      adapter.widgetState.options.at(-1)?.values.map((option) => option.value),
      ['none'],
    );
    adapter.widgetState.failure = null;
    port.widget(configuredWidgets());
    assert.equal(adapter.widgetState.timelines.at(-1)?.[0]?.props.entries.length, 2);
  }
});

test('preferences use only the current canonical file and never read an obsolete schema filename', async () => {
  adapter.fileState.paths = [];
  adapter.fileState.reads = [];
  adapter.fileState.records.clear();
  adapter.fileState.records.set('fixture/native-surfaces-v1.json', 'obsolete test record');
  const port = await adapter.createSurfacePort();
  assert.equal(await port.readPreferences(), null);
  assert.deepEqual(adapter.fileState.paths, ['fixture/native-surfaces.json']);
  assert.deepEqual(adapter.fileState.reads, []);
  const stored = JSON.stringify({
    schemaVersion: 2,
    owner: { identityId: 'id_a', deviceId: 'dev_a' },
    widgets: [],
    shortcutItems: [],
    activity: null,
  });
  await port.writePreferences(stored);
  assert.equal(await port.readPreferences(), stored);
  assert.deepEqual(adapter.fileState.reads, ['fixture/native-surfaces.json']);
  await port.writePreferences(null);
  assert.equal(await port.readPreferences(), null);
  assert.equal(
    adapter.fileState.records.get('fixture/native-surfaces-v1.json'),
    'obsolete test record',
  );
});
