import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { DeviceConsentProps } from './device-consent.types';
import type { useDeviceConsent } from './use-device-consent';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const rendererPath = fileURLToPath(new URL('./device-consent.ios.tsx', import.meta.url));
const hookPath = fileURLToPath(new URL('./use-device-consent.ts', import.meta.url));
const sdkPath = fileURLToPath(
  new URL('../../node_modules/@expo/ui/src/swift-ui/BottomSheet/index.tsx', import.meta.url),
);
const sources = new Map([
  [rendererPath, process.env.ZOEN_CONSENT_RENDERER_SOURCE ?? rendererPath],
  [hookPath, process.env.ZOEN_CONSENT_HOOK_SOURCE ?? hookPath],
]);
const ports = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'expo',
  '@expo/ui',
  '@expo/ui/swift-ui',
  '@expo/ui/swift-ui/modifiers',
  '../i18n',
  '../theme',
  '../../PresentedContentContext',
  '../SlotView',
  '../modifiers/utils',
]);
const output = await build({
  stdin: {
    contents: `export { DeviceConsent } from './device-consent.ios';
      export { useDeviceConsent } from './use-device-consent';
      export { BottomSheet as SDKBottomSheet } from ${JSON.stringify(sdkPath)};`,
    resolveDir: dirname(rendererPath),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'device-consent-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
        builder.onLoad(
          { filter: /(?:device-consent\.ios\.tsx|use-device-consent\.ts)$/ },
          ({ path }) => {
            const source = sources.get(path);
            assert.ok(source, `Unexpected consent source: ${path}`);
            return {
              contents: transformSync(readFileSync(source, 'utf8'), {
                filename: path,
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
              }).code,
              resolveDir: dirname(path),
              loader: 'js',
            };
          },
        );
      },
    },
  ],
});

type Element = { type: unknown; props: Record<string, unknown> };
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
type Instance = {
  slots: Slot[];
  cursor: number;
  caches: unknown[][];
  cacheCursor: number;
  effects: (() => void)[];
};
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...elements(node.props.children)];
}
const tick = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

/** Real compiled hook/renderer + installed SDK wrapper; only resources and native events are ports. */
function fixture() {
  const instances = new Map<string, Instance>();
  let active!: Instance;
  const nextSlot = () => (active.slots[active.cursor++] ??= {});
  const changed = (slot: Slot, deps?: readonly unknown[]) =>
    !slot.deps || !deps || deps.some((value, index) => !Object.is(value, slot.deps![index]));
  const appListeners = new Set<(state: string) => void>();
  const AppState = {
    currentState: 'active',
    addEventListener(_name: string, listener: (state: string) => void) {
      appListeners.add(listener);
      return { remove: () => appListeners.delete(listener) };
    },
  };
  const jsx = (type: unknown, props: Element['props']) => ({ type, props });
  const modules: Record<string, unknown> = {
    react: {
      useState(initial: unknown) {
        const slot = nextSlot();
        if (!('value' in slot)) slot.value = typeof initial === 'function' ? initial() : initial;
        return [
          slot.value,
          (value: unknown) => {
            slot.value = typeof value === 'function' ? value(slot.value) : value;
          },
        ];
      },
      useRef(initial: unknown) {
        const slot = nextSlot();
        return (slot.value ??= { current: initial });
      },
      useCallback(callback: unknown, deps: readonly unknown[]) {
        const slot = nextSlot();
        if (changed(slot, deps)) {
          slot.value = callback;
          slot.deps = deps;
        }
        return slot.value;
      },
      useSyncExternalStore(_subscribe: unknown, read: () => unknown) {
        return read();
      },
      useLayoutEffect(effect: Effect, deps?: readonly unknown[]) {
        const slot = nextSlot();
        if (changed(slot, deps))
          active.effects.push(() => {
            slot.cleanup?.();
            slot.cleanup = effect() || undefined;
            slot.deps = deps;
          });
      },
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (active.caches[active.cacheCursor++] ??= Array(size).fill(
          Symbol.for('react.memo_cache_sentinel'),
        )),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { AppState },
    expo: { requireNativeView: () => 'NativeBottomSheet' },
    '@expo/ui': { Host: 'Host' },
    '@expo/ui/swift-ui': {
      BottomSheet: 'Sheet',
      Button: 'Button',
      Group: 'Group',
      ScrollView: 'ScrollView',
      Text: 'Text',
      VStack: 'VStack',
    },
    '@expo/ui/swift-ui/modifiers': new Proxy(
      {},
      {
        get: (_object, name) =>
          name === 'shapes'
            ? { rectangle: () => ({ type: 'rectangle' }) }
            : (...args: unknown[]) => ({ name, args }),
      },
    ),
    '../i18n': { t: (key: string) => key },
    '../theme': {
      useTheme: () => ({
        scheme: 'dark',
        colors: {
          background: '#0D1421',
          text: '#FFFFFF',
          secondary: '#B0BAC6',
          accent: '#9BCD6A',
          onAccent: '#181D24',
        },
      }),
    },
    '../../PresentedContentContext': { PresentedContent: 'PresentedContent' },
    '../SlotView': { Slot: 'Slot' },
    '../modifiers/utils': { createViewModifierEventListener: () => ({}) },
  };
  const module = {
    exports: {} as {
      DeviceConsent(props: DeviceConsentProps): Element | null;
      useDeviceConsent(): ReturnType<typeof useDeviceConsent>;
      SDKBottomSheet(props: Record<string, unknown>): Element;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    Promise,
    require(name: string) {
      assert.ok(name in modules, `Unexpected consent import: ${name}`);
      return modules[name];
    },
  });
  const call = <T>(key: string, render: () => T): T => {
    let instance = instances.get(key);
    if (!instance) {
      instance = { slots: [], cursor: 0, caches: [], cacheCursor: 0, effects: [] };
      instances.set(key, instance);
    }
    active = instance;
    active.cursor = active.cacheCursor = 0;
    const value = render();
    for (const effect of active.effects.splice(0)) effect();
    return value;
  };
  let ownerId = 0;
  const owner = () => {
    const key = `owner-${++ownerId}`;
    const hook = () => call(key, () => module.exports.useDeviceConsent());
    const consent = hook();
    const render = () => {
      const current = hook();
      const tree = call(`${key}-renderer`, () =>
        module.exports.DeviceConsent({
          request: current.current,
          phase: current.phase,
          title: 'Synthetic app consent',
          scope: 'Synthetic chat only',
          onChoose: current.choose,
          onDismiss: current.dismissed,
        }),
      );
      const sheet = elements(tree).find((element) => element.type === 'Sheet');
      const native = sheet
        ? call(`${key}-sdk-${current.current!.id}`, () =>
            module.exports.SDKBottomSheet(sheet.props),
          )
        : undefined;
      const group = elements(tree).find((element) => element.type === 'Group');
      const appearance = (
        group?.props.modifiers as { name: string; args: unknown[] }[] | undefined
      )?.find((modifier) => modifier.name === 'onAppear');
      return {
        ...current,
        nativePresented: native?.props.isPresented,
        appeared() {
          assert.ok(appearance);
          (appearance.args[0] as () => void)();
        },
        choose(choice: 'once' | 'always' | 'deny') {
          const button = elements(tree).find(
            (element) => element.props.testID === `device-consent-${choice}`,
          );
          assert.ok(button);
          (button.props.onPress as () => void)();
        },
        presentationChanged(presented: boolean) {
          assert.equal(native?.type, 'NativeBottomSheet');
          (native.props.onIsPresentedChange as (event: unknown) => void)({
            nativeEvent: { isPresented: presented },
          });
        },
        dismissed() {
          assert.equal(native?.type, 'NativeBottomSheet');
          (native.props.onDismiss as () => void)();
        },
      };
    };
    return {
      consent,
      render,
      unmount() {
        for (const [name, instance] of instances)
          if (name === key || name.startsWith(`${key}-`))
            for (const slot of instance.slots) slot.cleanup?.();
      },
    };
  };
  return {
    owner,
    appState(state: string) {
      AppState.currentState = state;
      for (const listener of appListeners) listener(state);
    },
  };
}

test('actual native dismissal alone settles a staged choice; stale callbacks cannot authorize the next request', async () => {
  const app = fixture();
  const owner = app.owner();
  const accepted: string[] = [];
  void owner.consent
    .ask('calendar.freebusy', 'Synthetic calendar purpose')
    .then((choice) => accepted.push(`calendar:${choice}`));
  void owner.consent
    .ask('photos.pick', 'Synthetic photo purpose')
    .then((choice) => accepted.push(`photos:${choice}`));
  const first = owner.render();
  first.appeared();
  first.choose('once');
  first.choose('always');
  await tick();
  assert.deepEqual(accepted, []);
  assert.equal(owner.render().phase, 'dismissing');
  assert.equal(owner.render().nativePresented, false);
  first.presentationChanged(false);
  await tick();
  assert.deepEqual(accepted, [], 'binding changes are not UIKit dismissal completion');
  first.dismissed();
  await tick();
  assert.deepEqual(accepted, ['calendar:once']);
  const second = owner.render();
  second.appeared();
  assert.equal(second.current?.capability, 'photos.pick');
  first.dismissed();
  first.choose('always');
  await tick();
  assert.deepEqual(accepted, ['calendar:once']);
  assert.equal(owner.render().phase, 'presented');
  second.choose('always');
  owner.render().dismissed();
  await tick();
  assert.deepEqual(accepted, ['calendar:once', 'photos:always']);
  owner.unmount();
});

test('interactive native cancellation denies only after actual dismissal', async () => {
  const owner = fixture().owner();
  const accepted: string[] = [];
  void owner.consent
    .ask('calendar.freebusy', 'Synthetic purpose')
    .then((choice) => accepted.push(choice));
  const sheet = owner.render();
  sheet.appeared();
  sheet.presentationChanged(false);
  await tick();
  assert.deepEqual(accepted, []);
  assert.equal(owner.render().phase, 'dismissing');
  sheet.dismissed();
  await tick();
  assert.deepEqual(accepted, ['deny']);
  owner.unmount();
});

test('background cancellation denies active and queued work; foreground waits for the retiring native sheet', async () => {
  const app = fixture();
  const owner = app.owner();
  const accepted: string[] = [];
  void owner.consent
    .ask('calendar.freebusy', 'Synthetic calendar')
    .then((choice) => accepted.push(`calendar:${choice}`));
  void owner.consent
    .ask('photos.pick', 'Synthetic photos')
    .then((choice) => accepted.push(`photos:${choice}`));
  const old = owner.render();
  old.appeared();
  old.choose('once');
  app.appState('background');
  await tick();
  assert.deepEqual(accepted.sort(), ['calendar:deny', 'photos:deny']);
  assert.equal(await owner.consent.ask('location', 'Background must not prompt'), 'deny');
  app.appState('active');
  void owner.consent
    .ask('contacts.pick', 'New foreground purpose')
    .then((choice) => accepted.push(`contacts:${choice}`));
  assert.equal(owner.render().current?.id, old.current?.id);
  assert.equal(owner.render().phase, 'dismissing');
  old.dismissed();
  const fresh = owner.render();
  fresh.appeared();
  assert.equal(fresh.current?.capability, 'contacts.pick');
  old.choose('always');
  old.dismissed();
  assert.equal(owner.render().current?.id, fresh.current?.id);
  fresh.choose('deny');
  fresh.dismissed();
  await tick();
  assert.deepEqual(accepted.sort(), ['calendar:deny', 'contacts:deny', 'photos:deny']);
  owner.unmount();
});

test('retired hook owners deny late requests and their native callbacks cannot settle a new owner', async () => {
  const app = fixture();
  const first = app.owner();
  const accepted: string[] = [];
  void first.consent
    .ask('calendar.freebusy', 'Synthetic purpose')
    .then((choice) => accepted.push(choice));
  const old = first.render();
  old.appeared();
  old.choose('always');
  first.unmount();
  await tick();
  assert.deepEqual(accepted, ['deny']);
  assert.equal(await first.consent.ask('photos.pick', 'Retired request'), 'deny');
  const next = app.owner();
  const freshChoices: string[] = [];
  void next.consent
    .ask('calendar.freebusy', 'New owner')
    .then((choice) => freshChoices.push(choice));
  const current = next.render();
  current.appeared();
  assert.notEqual(current.current?.id, old.current?.id);
  old.dismissed();
  old.choose('once');
  await tick();
  assert.deepEqual(freshChoices, []);
  current.choose('once');
  current.dismissed();
  await tick();
  assert.deepEqual(freshChoices, ['once']);
  next.unmount();
});

test('cancellation before native appearance obtains a real disabled presentation before retiring its barrier', async () => {
  const app = fixture();
  const owner = app.owner();
  const decisions: string[] = [];
  void owner.consent
    .ask('calendar.freebusy', 'Synthetic never-shown request')
    .then((choice) => decisions.push(choice));
  app.appState('background');
  await tick();
  assert.deepEqual(decisions, ['deny']);
  const cancelled = owner.render();
  assert.equal(cancelled.phase, 'dismissing');
  assert.equal(
    cancelled.nativePresented,
    true,
    'a coalesced false must not strand a never-presented sheet',
  );
  app.appState('active');
  const freshDecisions: string[] = [];
  void owner.consent
    .ask('photos.pick', 'New foreground choice')
    .then((choice) => freshDecisions.push(choice));
  assert.equal(owner.render().current?.id, cancelled.current?.id);
  cancelled.choose('always');
  cancelled.appeared();
  const closing = owner.render();
  assert.equal(closing.nativePresented, false);
  assert.deepEqual(freshDecisions, []);
  closing.dismissed();
  const next = owner.render();
  next.appeared();
  assert.equal(next.current?.capability, 'photos.pick');
  cancelled.appeared();
  cancelled.dismissed();
  assert.equal(owner.render().current?.id, next.current?.id);
  next.choose('deny');
  assert.equal(
    owner.render().nativePresented,
    false,
    'late appearance from the old request cannot reopen the new sheet',
  );
  next.dismissed();
  await tick();
  assert.deepEqual(freshDecisions, ['deny']);
  owner.unmount();
});
