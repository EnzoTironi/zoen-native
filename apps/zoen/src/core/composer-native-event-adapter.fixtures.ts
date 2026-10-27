import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { transform } from 'esbuild';
import { require, transformSync } from './attachment-composer-ui.fixtures.ts';

const installed = (path: string) => new URL(`../../node_modules/${path}`, import.meta.url);
async function load(path: string, ports: Record<string, unknown>) {
  const file = installed(path);
  const { code } = await transform(readFileSync(file, 'utf8'), { loader: 'tsx', format: 'cjs' });
  const module = { exports: {} as Record<string, any> };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require(name: string) {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  return module.exports;
}

/** Execute installed adapters, including RN's actual _onChange body. Native
 * dispatch and the UI runtime are ports; this does not qualify device delivery. */
export async function nativeEventAdapter() {
  const registrations: { worklet: (...args: any[]) => unknown; event: string; tag: number }[] = [];
  const handlers = await load('react-native-reanimated/src/WorkletEventHandler.native.ts', {
    './core': {
      registerEventHandler(worklet: (...args: any[]) => unknown, event: string, tag: number) {
        registrations.push({ worklet, event, tag });
        return registrations.length;
      },
      unregisterEventHandler() {},
    },
  });
  const utils = await load('react-native-reanimated/src/createAnimatedComponent/utils.ts', {});
  const { PropsFilter } = await load(
    'react-native-reanimated/src/createAnimatedComponent/PropsFilter.tsx',
    {
      '../animation': { initialUpdaterRun: (read: () => unknown) => read() },
      '../css/utils': { isCSSConfigProp: () => false, isPseudoSelectorValue: () => false },
      '../isSharedValue': { isSharedValue: () => false },
      '../WorkletEventHandler': handlers,
      './InlinePropManager': { hasInlineStyles: () => false },
      './platform': {},
      './utils': utils,
    },
  );
  const { NativeEventsManager } = await load(
    'react-native-reanimated/src/createAnimatedComponent/NativeEventsManager.ts',
    {
      '../platform-specific/findHostInstance': { findHostInstance: () => ({ __nativeTag: 101 }) },
      '../platformFunctions/findNodeHandle': { findNodeHandle: () => 101 },
      '../WorkletEventHandler': handlers,
      './utils': utils,
    },
  );
  const rn = installed('react-native/Libraries/Components/TextInput/TextInput.js');
  const compiled = transformSync(readFileSync(rn, 'utf8'), {
    filename: rn.pathname,
    babelrc: false,
    configFile: false,
    presets: [require.resolve('babel-preset-expo')],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
    ast: true,
  }) as unknown as { ast: unknown };
  const traverse = require('@babel/traverse').default;
  const generate = require('@babel/generator').default;
  const callbacks: unknown[] = [];
  traverse(compiled.ast, {
    VariableDeclarator(path: { node: { id: { name?: string }; init: unknown } }) {
      if (path.node.id.name === '_onChange') callbacks.push(path.node.init);
    },
  });
  assert.equal(callbacks.length, 1, 'the installed RN adapter must have exactly one _onChange');
  const body = generate(callbacks[0]).code;
  return {
    registrations,
    connect(
      props: Record<string, unknown>,
      worklet: (...args: any[]) => unknown,
      eventName = 'onChange',
    ) {
      const component = {
        props: {
          onNativeChange: {
            workletEventHandler: new handlers.WorkletEventHandler(worklet, [eventName]),
          },
          ...props,
        },
        getComponentViewTag: () => 101,
      };
      const manager = new NativeEventsManager(component);
      manager.attachEvents();
      const filtered = new PropsFilter().filterNonAnimatedProps(component);
      const internal: { text: string; count: number } = { text: '', count: 0 };
      const change = runInNewContext(`(${body})`, {
        props: filtered,
        inputRef: { current: {} },
        setLastNativeText: (text: string) => {
          internal.text = text;
        },
        setMostRecentEventCount: (count: number) => {
          internal.count = count;
        },
      });
      return { filtered, internal, change, detach: () => manager.detachEvents() };
    },
  };
}
