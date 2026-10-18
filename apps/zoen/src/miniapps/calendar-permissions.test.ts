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
import { miniapps } from '../i18n/catalogs/miniapps.ts';

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
    noCalendars?: boolean;
    emptyEvents?: boolean;
    calendarGate?: Promise<void>;
    calendarEntered?: () => void;
    listGate?: Promise<void>;
    listEntered?: () => void;
    language?: 'en' | 'pt-BR';
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
  let events = [
    event(startMs - 1000, startMs + 1000),
    event(startMs + 500, startMs + 2000),
    event(startMs + 100, startMs + 5000, { availability: 'free' }),
    event(startMs, startMs + 86_400_000, { allDay: true }),
    event(endMs - 1000, endMs + 1000),
  ];
  const nativeCalendar = {
    EntityTypes: { EVENT: 'event' },
    Availability: {
      FREE: 'free',
      BUSY: 'busy',
      TENTATIVE: 'tentative',
      UNAVAILABLE: 'unavailable',
      NOT_SUPPORTED: 'notSupported',
    },
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
      options.calendarEntered?.();
      await options.calendarGate;
      return options.noCalendars ? [] : [calendar];
    },
    async listEvents(calendars: unknown[], start: Date, end: Date) {
      assert.deepEqual(calendars, options.noCalendars ? [] : [calendar]);
      calls.ranges.push({ startMs: start.getTime(), endMs: end.getTime() });
      options.listEntered?.();
      await options.listGate;
      if (options.failList) throw new Error('Synthetic calendar read failure');
      return options.noCalendars || options.emptyEvents ? [] : events;
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
    '../i18n/index.ts': {
      t: (key: string) =>
        key === 'miniapps.noCalendarsAvailable' && options.language
          ? miniapps[options.language].noCalendarsAvailable
          : key,
    },
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
  return {
    adapter,
    calls,
    startMs,
    endMs,
    setEvents(
      values: { startMs: number; endMs: number; allDay?: boolean; availability?: string }[],
    ) {
      events = values.map(({ startMs, endMs, ...extra }) => event(startMs, endMs, extra));
    },
    failProjection() {
      Object.defineProperty(events[0], 'startDate', {
        get: () => {
          throw new Error('Synthetic event getter failure');
        },
      });
    },
  };
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

test('no calendars reports localized setup guidance without querying events, then permits retry', async () => {
  for (const language of ['en', 'pt-BR'] as const) {
    const options = { language, noCalendars: true, emptyEvents: true };
    const { adapter, calls, startMs, endMs } = fixture(options);
    await assert.rejects(adapter.perform('calendar.freebusy', { startMs, endMs }, 'item'), {
      message: miniapps[language].noCalendarsAvailable,
    });
    assert.deepEqual(calls.ranges, [], 'absence is never reported as a successful empty agenda');
    assert.equal(calls.releasedCalendars, 0);
    assert.equal(calls.releasedEvents, 0);
    options.noCalendars = false;
    assert.deepEqual(
      serializable(await adapter.perform('calendar.freebusy', { startMs, endMs }, 'item')),
      [],
      'a real calendar with zero events is legitimately free',
    );
    assert.deepEqual(calls.permissions, [false, false]);
    assert.deepEqual(calls.ranges, [{ startMs, endMs }]);
    assert.equal(calls.releasedCalendars, 1);
  }
});

test('all-day busy, tentative and unavailable events block their full clipped intervals', async () => {
  for (const availability of ['busy', 'tentative', 'unavailable']) {
    const { adapter, calls, startMs, endMs, setEvents } = fixture();
    setEvents([
      { startMs: startMs - 1000, endMs: startMs + 86_400_000, allDay: true, availability },
      { startMs: endMs - 1000, endMs: endMs + 1000, allDay: true, availability },
    ]);
    assert.deepEqual(
      serializable(await adapter.perform('calendar.freebusy', { startMs, endMs }, 'item')),
      [
        { startMs, endMs: startMs + 86_400_000 },
        { startMs: endMs - 1000, endMs },
      ],
      availability,
    );
    assert.equal(calls.releasedEvents, 2);
    assert.equal(calls.releasedCalendars, 1);
  }
});

test('free and unsupported or unknown all-day notices do not block otherwise available days', async () => {
  const { adapter, calls, startMs, endMs, setEvents } = fixture();
  setEvents(
    ['free', 'notSupported', undefined, 'future-unknown'].map((availability) => ({
      startMs,
      endMs,
      allDay: true,
      availability,
    })),
  );
  assert.deepEqual(
    serializable(await adapter.perform('calendar.freebusy', { startMs, endMs }, 'item')),
    [],
  );
  assert.equal(calls.releasedEvents, 4);
  assert.equal(calls.releasedCalendars, 1);
});

test('timed non-free events preserve their conservative availability policy and merge adjacency', async () => {
  const { adapter, calls, startMs, endMs, setEvents } = fixture();
  const values = ['busy', 'tentative', 'unavailable', 'notSupported', undefined, 'future-unknown'];
  setEvents([
    ...values.map((availability, index) => ({
      startMs: startMs + index * 1000,
      endMs: startMs + (index + 1) * 1000,
      availability,
    })),
    { startMs, endMs, availability: 'free' },
  ]);
  assert.deepEqual(
    serializable(await adapter.perform('calendar.freebusy', { startMs, endMs }, 'item')),
    [{ startMs, endMs: startMs + 6000 }],
  );
  assert.equal(calls.releasedEvents, 7);
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

test('closing while native calendars are arriving releases them before any event query and permits reopen', async () => {
  let entered!: () => void;
  let resume!: () => void;
  const calendarEntered = new Promise<void>((resolve) => (entered = resolve));
  const calendarGate = new Promise<void>((resolve) => (resume = resolve));
  const { adapter, calls, startMs, endMs } = fixture({
    calendarEntered: entered,
    calendarGate,
    emptyEvents: true,
  });
  const pending = adapter.perform('calendar.freebusy', { startMs, endMs }, 'item');
  await calendarEntered;
  adapter.releaseItemPhotos('item');
  resume();
  await assert.rejects(pending, /thisMiniappHasClosed/);
  assert.deepEqual(calls.ranges, []);
  assert.equal(calls.releasedCalendars, 1);
  assert.deepEqual(
    serializable(await adapter.perform('calendar.freebusy', { startMs, endMs }, 'item')),
    [],
  );
  assert.equal(calls.releasedCalendars, 2);
});

test('late event results after lease retirement release every native resource and cannot publish availability', async () => {
  let entered!: () => void;
  let resume!: () => void;
  const listEntered = new Promise<void>((resolve) => (entered = resolve));
  const listGate = new Promise<void>((resolve) => (resume = resolve));
  const { adapter, calls, startMs, endMs } = fixture({ listEntered: entered, listGate });
  const pending = adapter.perform('calendar.freebusy', { startMs, endMs }, 'item');
  await listEntered;
  adapter.releaseItemPhotos('item');
  resume();
  await assert.rejects(pending, /thisMiniappHasClosed/);
  assert.equal(calls.releasedEvents, 5);
  assert.equal(calls.releasedCalendars, 1);
  const saved = await adapter.perform(
    'calendar.add',
    { title: 'Retry after closed reader', startMs, endMs: startMs + 1000 },
    'item',
  );
  assert.deepEqual(serializable(saved), { saved: true });
});

test('projection exceptions release every event and calendar instead of leaking native handles', async () => {
  const { adapter, calls, startMs, endMs, failProjection, setEvents } = fixture();
  failProjection();
  await assert.rejects(
    adapter.perform('calendar.freebusy', { startMs, endMs }, 'item'),
    /Synthetic event getter failure/,
  );
  assert.equal(calls.releasedEvents, 5);
  assert.equal(calls.releasedCalendars, 1);
  setEvents([]);
  assert.deepEqual(
    serializable(await adapter.perform('calendar.freebusy', { startMs, endMs }, 'item')),
    [],
  );
  assert.equal(calls.releasedCalendars, 2);
});
