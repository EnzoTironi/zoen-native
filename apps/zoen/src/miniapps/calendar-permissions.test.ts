import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import type { ExpoConfig } from 'expo/config';
import type { ConfigPlugin, ExportedConfig } from 'expo/config-plugins';
import { build } from 'esbuild';
import type * as NativeCapabilities from './native-capabilities';

const require = createRequire(import.meta.url);
const projectRoot = fileURLToPath(new URL('../..', import.meta.url));
const appPath = process.env.ZOEN_CALENDAR_APP_CONFIG ?? new URL('../../app.json', import.meta.url);
const app = JSON.parse(readFileSync(appPath, 'utf8')).expo as ExpoConfig;
const { default: withCalendar } = require('expo-calendar/app.plugin.js') as {
  default: ConfigPlugin<Record<string, string | boolean>>;
};
const { getResolvedLocalesAsync } = require('@expo/config-plugins/build/utils/locales') as {
  getResolvedLocalesAsync(
    root: string,
    locales: NonNullable<ExpoConfig['locales']>,
    platform: 'ios',
  ): Promise<{ localesMap: Record<string, Record<string, string>> }>;
};
const calendarPlugin = app.plugins?.find(
  (plugin) => Array.isArray(plugin) && plugin[0] === 'expo-calendar',
);
assert.ok(Array.isArray(calendarPlugin), 'Exercise the actual app calendar plugin configuration');
const configured = withCalendar(
  structuredClone(app),
  calendarPlugin[1] as Record<string, string | boolean>,
) as ExportedConfig;
assert.ok(configured.mods?.ios?.infoPlist);
const { modResults: infoPlist } = await configured.mods.ios.infoPlist({
  ...configured,
  modResults: { ...app.ios?.infoPlist },
  modRawConfig: app,
  modRequest: {
    projectRoot,
    platformProjectRoot: projectRoot,
    platform: 'ios',
    modName: 'infoPlist',
    introspect: true,
  },
});

// Execute the production adapter and busy-block projection; replace only native resources.
const ports = new Set([
  'expo',
  'expo-device',
  'expo-file-system',
  'expo-image-picker',
  'expo-image-manipulator',
  'expo-crypto',
  'expo-calendar',
  'expo-location',
  'expo-contacts/legacy',
  'react-native',
  '../i18n/index.ts',
  './photo-preview',
]);
const built = await build({
  entryPoints: [fileURLToPath(new URL('./native-capabilities.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  supported: { 'dynamic-import': false },
  write: false,
  plugins: [
    {
      name: 'calendar-native-resources',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
type CalendarEvent = {
  startDate: Date;
  endDate: Date;
  allDay?: boolean;
  availability?: string;
  release(): void;
};
function fixture(
  options: {
    denied?: boolean;
    permissionGate?: Promise<void>;
    permissionEntered?: () => void;
    failList?: boolean;
  } = {},
) {
  const calls = {
    permissions: [] as boolean[],
    calendars: 0,
    ranges: [] as { startMs: number; endMs: number }[],
    releasedCalendars: 0,
    releasedEvents: 0,
    addForms: 0,
  };
  const startMs = Date.UTC(2026, 9, 18);
  const endMs = startMs + 31 * 86_400_000;
  const calendar = { release: () => calls.releasedCalendars++ };
  const event = (start: number, end: number, extra: Partial<CalendarEvent> = {}): CalendarEvent =>
    Object.defineProperties(
      {
        startDate: new Date(start),
        endDate: new Date(end),
        release: () => calls.releasedEvents++,
        ...extra,
      },
      Object.fromEntries(
        ['title', 'notes', 'location', 'attendees', 'calendarId', 'id'].map((name) => [
          name,
          { get: () => assert.fail(`Free/busy must not read private event field ${name}`) },
        ]),
      ),
    );
  const events = [
    event(startMs - 1000, startMs + 1000),
    event(startMs + 500, startMs + 2000),
    event(startMs + 100, startMs + 5000, { availability: 'free' }),
    event(startMs, startMs + 86_400_000, { allDay: true }),
    event(endMs - 1000, endMs + 1000),
  ];
  const nativeCalendar = {
    EntityTypes: { EVENT: 'event' },
    Availability: { FREE: 'free' },
    CalendarDialogResultActions: { saved: 'saved' },
    async requestCalendarPermissions(writeOnly: boolean) {
      calls.permissions.push(writeOnly);
      const key = writeOnly
        ? 'NSCalendarsWriteOnlyAccessUsageDescription'
        : 'NSCalendarsFullAccessUsageDescription';
      assert.equal(typeof infoPlist[key], 'string', `${key} must exist in actual plugin output`);
      options.permissionEntered?.();
      await options.permissionGate;
      return { granted: !options.denied };
    },
    async getCalendars(entity: string) {
      assert.equal(entity, 'event');
      calls.calendars++;
      return [calendar];
    },
    async listEvents(calendars: unknown[], start: Date, end: Date) {
      assert.deepEqual(calendars, [calendar]);
      calls.ranges.push({ startMs: start.getTime(), endMs: end.getTime() });
      if (options.failList) throw new Error('Synthetic calendar read failure');
      return events;
    },
    getDefaultCalendarSync: () => ({
      async addEventWithForm() {
        calls.addForms++;
        return { action: 'saved' };
      },
      release: () => calls.releasedCalendars++,
    }),
  };
  const mocks: Record<string, unknown> = {
    expo: { requireOptionalNativeModule: (name: string) => (name === 'ExpoCalendar' ? {} : null) },
    'expo-device': { isDevice: false },
    'expo-file-system': { Paths: { cache: { uri: 'file:///synthetic-cache/' } } },
    'expo-calendar': nativeCalendar,
    'react-native': {
      Platform: { OS: 'ios' },
      AppState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
    },
    '../i18n/index.ts': { t: (key: string) => key },
    './photo-preview': {},
  };
  const module = { exports: {} };
  runInNewContext(built.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    AbortController,
    Date,
    require(path: string) {
      assert.ok(path in mocks, `Unexpected native resource: ${path}`);
      return mocks[path];
    },
  });
  const adapter = module.exports as typeof NativeCapabilities;
  return { adapter, calls, startMs, endMs };
}
const serializable = (value: unknown): unknown => JSON.parse(JSON.stringify(value));

test('the actual Calendar plugin supports read and write-only prompts without reminder access', () => {
  for (const key of ['NSCalendarsUsageDescription', 'NSCalendarsFullAccessUsageDescription']) {
    assert.equal(typeof infoPlist[key], 'string', `${key} enables the real full-read requester`);
    assert.match(
      infoPlist[key] as string,
      /read your calendars.*busy time blocks.*without event names/,
    );
  }
  assert.equal(typeof infoPlist.NSCalendarsWriteOnlyAccessUsageDescription, 'string');
  assert.equal(infoPlist.NSRemindersUsageDescription, undefined);
  assert.equal(infoPlist.NSRemindersFullAccessUsageDescription, undefined);
});

test('Expo resolves English and Portuguese permission metadata without losing write-only localization', async () => {
  assert.ok(app.locales);
  const { localesMap } = await getResolvedLocalesAsync(projectRoot, app.locales, 'ios');
  for (const language of ['en', 'pt-BR']) {
    const strings = localesMap[language]!;
    assert.equal(typeof strings.NSCalendarsFullAccessUsageDescription, 'string');
    assert.equal(
      strings.NSCalendarsUsageDescription,
      strings.NSCalendarsFullAccessUsageDescription,
    );
    assert.equal(typeof strings.NSCalendarsWriteOnlyAccessUsageDescription, 'string');
  }
  assert.equal(
    localesMap.en!.NSCalendarsFullAccessUsageDescription,
    infoPlist.NSCalendarsFullAccessUsageDescription,
  );
  assert.match(
    localesMap['pt-BR']!.NSCalendarsFullAccessUsageDescription!,
    /horários livres.*intervalos ocupados.*sem nomes ou detalhes/,
  );
});

test('freebusy requests access only when used and returns clipped merged intervals with no event details', async () => {
  const { adapter, calls, startMs, endMs } = fixture();
  assert.ok(adapter.available().includes('calendar.freebusy'));
  assert.deepEqual(calls.permissions, []);
  await assert.rejects(
    adapter.perform('calendar.freebusy', { startMs, endMs: startMs }, 'item'),
    /chooseAValidDateRange/,
  );
  assert.deepEqual(calls.permissions, []);
  const result = await adapter.perform(
    'calendar.freebusy',
    { startMs, endMs: endMs + 86_400_000 },
    'item',
  );
  assert.deepEqual(calls.permissions, [false]);
  assert.deepEqual(calls.ranges, [{ startMs, endMs }]);
  assert.deepEqual(serializable(result), [
    { startMs, endMs: startMs + 2000 },
    { startMs: endMs - 1000, endMs },
  ]);
  assert.equal(calls.releasedEvents, 5);
  assert.equal(calls.releasedCalendars, 1);
});

test('denied read permission never queries calendars or fabricates free time', async () => {
  const { adapter, calls, startMs, endMs } = fixture({ denied: true });
  await assert.rejects(
    adapter.perform('calendar.freebusy', { startMs, endMs }, 'item'),
    /allowCalendarReading/,
  );
  assert.deepEqual(calls.permissions, [false]);
  assert.equal(calls.calendars, 0);
  assert.deepEqual(calls.ranges, []);
});

test('closing a miniapp during the permission prompt prevents a late calendar read', async () => {
  let entered!: () => void;
  let resume!: () => void;
  const permissionEntered = new Promise<void>((resolve) => (entered = resolve));
  const permissionGate = new Promise<void>((resolve) => (resume = resolve));
  const { adapter, calls, startMs, endMs } = fixture({
    permissionGate,
    permissionEntered: entered,
  });
  const pending = adapter.perform('calendar.freebusy', { startMs, endMs }, 'item');
  await permissionEntered;
  adapter.releaseItemPhotos('item');
  resume();
  await assert.rejects(pending, /thisMiniappHasClosed/);
  assert.equal(calls.calendars, 0);
  assert.deepEqual(calls.ranges, []);
});

test('calendar query failure releases its native calendar objects and permits a later action', async () => {
  const { adapter, calls, startMs, endMs } = fixture({ failList: true });
  await assert.rejects(
    adapter.perform('calendar.freebusy', { startMs, endMs }, 'item'),
    /Synthetic calendar read failure/,
  );
  assert.equal(calls.releasedCalendars, 1);
  const saved = await adapter.perform(
    'calendar.add',
    { title: 'Walk', startMs, endMs: startMs + 1000 },
    'item',
  );
  assert.deepEqual(serializable(saved), { saved: true });
  assert.deepEqual(calls.permissions, [false, true]);
  assert.equal(calls.addForms, 1);
  assert.equal(calls.calendars, 1);
});
