import assert from 'node:assert/strict';
import * as themeInputProbe from '../theme/input-probe.ts';
import { nativeSwitch } from './theme-native-switch.fixtures.ts';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import test from 'node:test';
import type { useTheme } from '../theme';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import {
  DEFAULT_THEME_CUSTOMIZATION as defaults,
  telegramPresentation,
} from '../theme/telegram.ts';
import { THEME_PROFILES, isDarkTheme } from '../theme/catalog.ts';
import { resolveSkyTheme } from '../theme/sky.ts';

const require = createRequire(import.meta.url);
const source = fileURLToPath(new URL('../theme.tsx', import.meta.url));
const compiled = require('@babel/core').transformSync(readFileSync(source, 'utf8'), {
  filename: source,
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
  caller: {
    name: 'metro',
    platform: 'ios',
    engine: 'hermes',
    isDev: false,
    supportsReactCompiler: true,
  },
}).code;
const ports = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  './preferences',
  './theme/input-probe',
  './i18n/index.ts',
]);
const output = await build({
  stdin: { contents: compiled, resolveDir: dirname(source), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'actual-theme-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
const settingsOutput = await build({
  stdin: {
    contents: readFileSync(new URL('../components/theme-settings.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  platform: 'node',
  format: 'cjs',
  bundle: true,
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'actual-settings-control-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
type Element = { type: unknown; props: Record<string, any> };
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function fixture(probeEnabled = false) {
  let cursor = 0,
    compilerCursor = 0;
  const traces: string[] = [];
  const probe = {
    ...themeInputProbe,
    THEME_INPUT_PROBE: probeEnabled,
    reportThemeInput: themeInputProbe.createThemeInputProbe(probeEnabled, (line) =>
      traces.push(line),
    ),
  };
  const hooks: unknown[] = [],
    caches: unknown[][] = [],
    effects: (() => unknown)[] = [];
  const reads = new Map<string, ReturnType<typeof deferred<string | null>>>();
  const writes: [string, string][] = [];
  const failed = new Set<string>();
  const saved = new Map<string, string>();
  const element = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const useRef = (value: unknown) => (hooks[cursor++] ??= { current: value });
  const modules: Record<string, unknown> = {
    react: {
      createContext: () => 'ThemeContext',
      use() {},
      useCallback: (value: unknown, deps: unknown[]) => {
        const slot = cursor++;
        const before = hooks[slot] as { deps: unknown[]; value: unknown } | undefined;
        if (!before || !deps.every((dep, index) => dep === before.deps[index]))
          hooks[slot] = { deps, value };
        return (hooks[slot] as { value: unknown }).value;
      },
      useRef,
      useMemo: (read: () => unknown) => {
        cursor++;
        return read();
      },
      useState: (initial: unknown) => {
        const slot = cursor++;
        if (!(slot in hooks)) hooks[slot] = typeof initial === 'function' ? initial() : initial;
        return [
          hooks[slot],
          (value: unknown) => {
            hooks[slot] = typeof value === 'function' ? value(hooks[slot]) : value;
          },
        ];
      },
      useEffect: (effect: () => unknown, deps: unknown[]) => {
        const slot = cursor++;
        const before = hooks[slot] as unknown[] | undefined;
        if (!before || !deps.every((value, index) => value === before[index])) {
          hooks[slot] = deps;
          effects.push(effect);
        }
      },
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (caches[compilerCursor++] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    'react/jsx-runtime': { jsx: element, jsxs: element },
    'react-native': {
      Platform: { OS: 'ios' },
      Appearance: { setColorScheme() {} },
      AppState: { currentState: 'background', addEventListener: () => ({ remove() {} }) },
      AccessibilityInfo: {
        addEventListener: () => ({ remove() {} }),
        isReduceMotionEnabled: async () => false,
      },
    },
    './theme/input-probe': probe,
    './i18n/index.ts': { t: (key: string) => key },
    './preferences': {
      preferences: {
        get: (key: string) => {
          const task = deferred<string | null>();
          reads.set(key, task);
          return task.promise;
        },
        set: async (key: string, value: string) => {
          writes.push([key, value]);
          if (failed.has(key)) throw new Error('simulated device storage failure');
          saved.set(key, value);
        },
      },
    },
  };
  const module = {
    exports: {} as { ThemeProvider(props: { children: null }): { props: { value: unknown } } },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), name);
      return modules[name];
    },
    setInterval: () => {
      throw new Error('background must not start the clock');
    },
    clearInterval() {},
  });
  type Theme = ReturnType<typeof useTheme>;
  function draw(): Theme {
    cursor = 0;
    compilerCursor = 0;
    const result = module.exports.ThemeProvider({ children: null });
    for (const effect of effects.splice(0)) effect();
    return result.props.value as Theme;
  }
  const settle = async () => {
    await new Promise<void>((done) => setImmediate(done));
    return draw();
  };
  let currentTheme: Theme;
  let editing: unknown;
  const settingsModules: Record<string, unknown> = {
    react: {
      useState: (initial: unknown) => {
        editing ??= initial;
        return [
          editing,
          (next: unknown) => {
            editing = next;
          },
        ];
      },
    },
    'react/jsx-runtime': { jsx: element, jsxs: element },
    'react-native': {
      Pressable: 'Pressable',
      ScrollView: 'ScrollView',
      Text: 'Text',
      View: 'View',
    },
    '@expo/ui': { Host: 'Host', Switch: 'Switch', Slider: 'Slider' },
    '@expo/ui/swift-ui/modifiers': Object.fromEntries(
      [
        'accessibilityLabel',
        'accessibilityValue',
        'accessibilityHint',
        'foregroundStyle',
        'frame',
      ].map((name) => [name, (value: unknown) => ({ name, value })]),
    ),
    '../theme/input-probe': probe,
    '../i18n': { t: (key: string) => key },
    '../theme': { useTheme: () => currentTheme },
    '../theme/sky': { resolveSkyTheme },
    '../theme/catalog': { THEME_PROFILES, isDarkTheme },
    '../theme/telegram': { DEFAULT_THEME_CUSTOMIZATION: defaults, telegramPresentation },
    './settings-screen': { SettingsScreen: 'SettingsScreen' },
    './sky-background': { SkyLayer: 'SkyLayer' },
    './theme-color-picker': { ThemeColorPicker: 'ThemeColorPicker' },
  };
  const settingsModule = { exports: {} as { ThemeSettings(): Element } };
  runInNewContext(settingsOutput.outputFiles[0]!.text, {
    module: settingsModule,
    exports: settingsModule.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require: (name: string) => {
      assert.ok(Object.hasOwn(settingsModules, name), name);
      return settingsModules[name];
    },
  });
  function drawSettings() {
    currentTheme = draw();
    return settingsModule.exports.ThemeSettings();
  }
  function automaticSwitch() {
    const row = nodes(drawSettings()).find(
      (node) => node.props.label === 'settings.themeAutomatic',
    )!;
    assert.equal(typeof row.type, 'function');
    const host = (row.type as (props: Record<string, any>) => Element)(row.props);
    const control = nodes(host).find((node) => node.type === 'Switch')!;
    return nativeSwitch(control.props);
  }
  return { draw, drawSettings, automaticSwitch, settle, reads, writes, failed, saved, traces };
}

test('successful Night/appearance writes never hide an unsaved Day customization', async () => {
  const f = fixture();
  const first = f.draw();
  f.failed.add('theme-customization-day');
  first.setCustomization('day', { ...defaults, accent: '#123456' });
  let current = await f.settle();
  assert.ok(current.preferenceError);
  current.setCustomization('night', { ...defaults, textSize: 23 });
  current = await f.settle();
  assert.ok(current.preferenceError, 'Night saving cannot acknowledge a failed Day write');
  current.setPreference('night');
  current = await f.settle();
  assert.ok(current.preferenceError, 'appearance saving cannot acknowledge a failed Day write');
  f.failed.delete('theme-customization-day');
  current.setCustomization('day', { ...defaults, accent: '#123456' });
  current = await f.settle();
  assert.equal(current.preferenceError, null);
  assert.equal(JSON.parse(f.saved.get('theme-customization-day')!).accent, '#123456');
});

test('late hydration cannot replace a user choice and coalesced writes persist the latest font', async () => {
  const f = fixture();
  const first = f.draw();
  first.setPreference('day');
  first.setCustomization('day', { ...defaults, textSize: 19 });
  first.setCustomization('day', { ...defaults, textSize: 26 });
  f.reads.get('appearance')!.resolve('night');
  f.reads.get('theme-customization-day')!.resolve(JSON.stringify({ ...defaults, textSize: 13 }));
  const current = await f.settle();
  assert.equal(current.preference, 'day');
  assert.equal(current.customizations.day.textSize, 26);
  assert.equal(current.presentation.typography.message.fontSize, 26);
  assert.equal(JSON.parse(f.saved.get('theme-customization-day')!).textSize, 26);
});

test('appearance readiness waits for every preset read, including rejection, without replacing a newer user choice', async () => {
  for (const rejected of [
    null,
    'appearance',
    ...THEME_PROFILES.map((profile) => `theme-customization-${profile}`),
  ]) {
    const f = fixture();
    const initial = f.draw();
    assert.equal(initial.appearanceReady, false);
    assert.equal(f.reads.size, THEME_PROFILES.length + 1);
    initial.setPreference('day');
    initial.setCustomization('day', { ...defaults, textSize: 23 });
    const complete = (key: string, value: string | null) => {
      const read = f.reads.get(key)!;
      if (key === rejected) read.reject(new Error('synthetic preference read failure'));
      else read.resolve(value);
    };
    complete('appearance', 'night');
    assert.equal(
      (await f.settle()).appearanceReady,
      false,
      'one settled read cannot release launch appearance',
    );
    complete('theme-customization-day', JSON.stringify({ ...defaults, textSize: 13 }));
    assert.equal(
      (await f.settle()).appearanceReady,
      false,
      'two settled reads still cannot release launch appearance',
    );
    for (const profile of THEME_PROFILES.filter((value) => value !== 'day'))
      complete(`theme-customization-${profile}`, JSON.stringify(defaults));
    const current = await f.settle();
    assert.equal(current.appearanceReady, true, String(rejected));
    assert.equal(current.preference, 'day');
    assert.equal(current.customizations.day.textSize, 23);
    assert.equal(current.presentation.typography.message.fontSize, 23);
  }
});

test('same-render native font and color callbacks merge fields in actual Settings and survive hydration/persistence', async () => {
  for (const order of ['font-first', 'color-first'] as const) {
    const f = fixture();
    f.draw().setPreference('day');
    await f.settle();
    const tree = f.drawSettings();
    const font = nodes(tree).find((node) => node.props.testID === 'theme-text-size')!;
    const accent = nodes(tree).find((node) => node.props.label === 'settings.themeAccent')!;
    assert.ok(font && accent);
    if (order === 'font-first') {
      font.props.onValueChange(23);
      accent.props.onChange('#123456');
    } else {
      accent.props.onChange('#123456');
      font.props.onValueChange(23);
    }
    // Both callbacks retain the same rendered native-control owner; no intervening draw.
    f.reads
      .get('theme-customization-day')!
      .resolve(JSON.stringify({ ...defaults, textSize: 13, accent: '#FFFFFF' }));
    f.reads
      .get('theme-customization-night')!
      .resolve(JSON.stringify({ ...defaults, textSize: 19 }));
    const current = await f.settle();
    assert.equal(current.customizations.day.textSize, 23, order);
    assert.equal(current.customizations.day.accent, '#123456', order);
    assert.equal(current.customizations.night.textSize, 19, 'unmodified family still hydrates');
    const saved = JSON.parse(f.saved.get('theme-customization-day')!);
    assert.equal(saved.textSize, 23);
    assert.equal(saved.accent, '#123456');
    assert.equal(current.preferenceError, null);
  }
});

test('all preset controls switch the actual provider and retain independent customization through late hydration', async () => {
  const f = fixture();
  f.draw().setPreference('day');
  await f.settle();
  const expected = new Map<string, number>();
  for (const [index, profile] of THEME_PROFILES.entries()) {
    const tree = f.drawSettings();
    const preset = nodes(tree).find((node) => node.props.profile === profile)!;
    assert.ok(preset, profile);
    preset.props.onPress();
    let current = await f.settle();
    assert.equal(current.preference, profile);
    assert.equal(current.profile, profile);
    assert.equal(current.presentation.profile, profile);
    const font = nodes(f.drawSettings()).find((node) => node.props.testID === 'theme-text-size')!;
    font.props.onValueChange(18 + index);
    expected.set(profile, 18 + index);
    // A stale disk response for this same theme cannot overwrite the native edit.
    f.reads
      .get(`theme-customization-${profile}`)!
      .resolve(JSON.stringify({ ...defaults, textSize: 13 }));
    current = await f.settle();
    assert.equal(current.presentation.typography.message.fontSize, 18 + index);
    for (const [other, size] of expected) {
      assert.equal(
        current.customizations[other as keyof typeof current.customizations].textSize,
        size,
      );
      assert.equal(JSON.parse(f.saved.get(`theme-customization-${other}`)!).textSize, size);
    }
  }
  f.reads.get('appearance')!.resolve('day');
  const final = await f.settle();
  assert.equal(final.preference, 'telegram-tinted');
  assert.equal(final.appearanceReady, true);
});

test('cold provider restoration recovers each selected preset and its own saved colors without writing migration data', async () => {
  for (const profile of THEME_PROFILES) {
    const f = fixture();
    f.draw();
    f.reads.get('appearance')!.resolve(profile);
    for (const other of THEME_PROFILES)
      f.reads.get(`theme-customization-${other}`)!.resolve(
        JSON.stringify({
          ...defaults,
          textSize: other === profile ? 23 : 17,
          outgoing: other === profile ? '#123456' : null,
        }),
      );
    const restored = await f.settle();
    assert.equal(restored.appearanceReady, true);
    assert.equal(restored.profile, profile);
    assert.equal(restored.presentation.chat.outgoing.fill, '#123456');
    assert.equal(restored.presentation.typography.message.fontSize, 23);
    assert.equal(f.writes.length, 0, 'restoration must not mutate any existing preference');
  }
});

test('installed iOS Switch native event reaches actual Automatic preference and persists both directions', async () => {
  const f = fixture();
  f.draw().setPreference('night');
  await f.settle();
  const off = f.automaticSwitch();
  assert.equal(off.props.isOn, false);
  off.props.onIsOnChange({ nativeEvent: { isOn: true } });
  let current = await f.settle();
  assert.equal(current.preference, 'auto');
  assert.equal(f.saved.get('appearance'), 'auto');
  assert.equal(f.automaticSwitch().props.isOn, true);
  // An obsolete disk read must not undo the native event.
  f.reads.get('appearance')!.resolve('day');
  current = await f.settle();
  assert.equal(current.preference, 'auto');
  f.automaticSwitch().props.onIsOnChange({ nativeEvent: { isOn: false } });
  current = await f.settle();
  assert.equal(current.preference, 'night');
  assert.equal(f.saved.get('appearance'), 'night');
});

test('opt-in Automatic probe observes callback and actual provider commit without changing selection or storing diagnostics', async () => {
  for (const enabled of [false, true]) {
    const f = fixture(enabled);
    f.draw().setPreference('night');
    await f.settle();
    f.traces.length = 0;
    const screen = f.drawSettings();
    const control = f.automaticSwitch();
    assert.equal(control.props.testID, enabled ? 'zoen-theme-automatic-probe' : undefined);
    assert.equal(typeof screen.props.onTouchStart, enabled ? 'function' : 'undefined');
    screen.props.onTouchStart?.();
    control.props.onIsOnChange({ nativeEvent: { isOn: true } });
    const current = await f.settle();
    assert.equal(current.preference, 'auto');
    assert.equal(f.saved.get('appearance'), 'auto');
    assert.deepEqual(
      [...f.saved.keys()],
      ['appearance'],
      'probe never creates preference or diagnostic records',
    );
    const stages = f.traces.map((line) =>
      line.replace(/^\[ZoenThemeInput\] /, '').replace(/ count=\d+$/, ''),
    );
    assert.deepEqual(
      stages,
      enabled
        ? ['screen-touch-start', 'js-callback', 'preference-request', 'preference-committed']
        : [],
    );
  }
});
