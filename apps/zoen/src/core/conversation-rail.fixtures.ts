import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import * as geometry from './conversation-rail-motion.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import type {
  ConversationRailMotion,
  useConversationRail,
} from '../components/use-conversation-rail';
import type { ConversationRail, ConversationRailSpacer } from '../components/conversation-rail';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const names = ['use-conversation-rail.ts', 'conversation-rail.tsx'];
const compiled = new Map(
  names.map((name) => {
    const file = new URL(`../components/${name}`, import.meta.url);
    return [
      name,
      transformSync(readFileSync(file, 'utf8'), {
        filename: file.pathname,
        babelrc: false,
        configFile: false,
        presets: ['babel-preset-expo'],
        caller: {
          name: 'metro',
          platform: 'ios',
          engine: 'hermes',
          isDev: false,
          supportsReactCompiler: true,
        },
      }).code,
    ];
  }),
);

class Shared<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
  get() {
    return this.value;
  }
  set(next: T) {
    this.value = next;
  }
}

/** Actual Expo/React Compiler output and React lifecycle; native frames are ports. */
export async function conversationRailFixture() {
  const root = createReactTestRoot();
  const keyboard = new Shared(0);
  let options = { scope: 'owner:chat', height: 180, collapsible: true, reduced: false };
  let current: ConversationRailMotion;
  let mounts = 0;
  let unmounts = 0;
  const frames: { target: number; config: unknown }[] = [];
  const scheduled: (() => void)[] = [];
  const views = new Map<string, Record<string, any>>();
  const view = (props: Record<string, any>) => {
    if (props.testID) views.set(props.testID, props);
    return React.createElement(React.Fragment, null, props.children);
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': { View: view, Pressable: view },
    'expo-haptics': { selectionAsync: async () => {} },
    'react-native-worklets': {
      scheduleOnRN: (work: (...args: any[]) => void, ...args: any[]) =>
        scheduled.push(() => work(...args)),
    },
    'react-native-gesture-handler': {
      usePanGesture: (config: unknown) => config,
      GestureDetector: ({ children }: { children: React.ReactNode }) => children,
    },
    'react-native-keyboard-controller': {
      useKeyboardState: (select: (value: unknown) => unknown) =>
        select({ isVisible: keyboard.value > 0 }),
    },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: view },
      cancelAnimation() {},
      useSharedValue: <T>(value: T) => React.useState(() => new Shared(value))[0],
      useDerivedValue: (read: () => number) => {
        const latest = React.useRef(read);
        latest.current = read;
        return React.useState(() => ({
          get value() {
            return latest.current();
          },
          get: () => latest.current(),
        }))[0];
      },
      useAnimatedStyle: (read: () => unknown) => read(),
      withSpring: (target: number, config: unknown) => {
        frames.push({ target, config });
        return target;
      },
    },
    './keyboard-motion': { useKeyboardMotion: () => ({ progress: keyboard }) },
    '../theme': {
      useTheme: () => ({
        reducedMotion: options.reduced,
        colors: { secondary: '#888888' },
        presentation: telegramPresentation('night'),
      }),
    },
    '../i18n': { t: (key: string) => key },
    '../core/conversation-rail-motion': geometry,
  };
  function load<T>(name: string): T {
    const module = { exports: {} };
    runInNewContext(compiled.get(name)!, {
      module,
      exports: module.exports,
      require(path: string) {
        if (path.startsWith('@babel/runtime/')) return require(path);
        assert.ok(path in ports, `Missing native rail port ${path}`);
        return ports[path];
      },
      queueMicrotask,
      global: { Error },
      __DEV__: false,
    });
    return module.exports as T;
  }
  const hook = load<{ useConversationRail: typeof useConversationRail }>(
    'use-conversation-rail.ts',
  );
  const component = load<{
    ConversationRail: typeof ConversationRail;
    ConversationRailSpacer: typeof ConversationRailSpacer;
  }>('conversation-rail.tsx');
  function Cards() {
    React.useEffect(() => {
      mounts++;
      return () => {
        unmounts++;
      };
    }, []);
    return null;
  }
  function Harness() {
    current = hook.useConversationRail(options.scope, options.height, options.collapsible);
    return React.createElement(
      React.Fragment,
      null,
      React.createElement(
        component.ConversationRail,
        {
          height: options.height,
          onHeight() {},
          motion: current,
          collapsible: options.collapsible,
        },
        React.createElement(Cards),
      ),
      React.createElement(component.ConversationRailSpacer, { motion: current }),
    );
  }
  async function render(next: Partial<typeof options> = {}) {
    options = { ...options, ...next };
    views.clear();
    await root.render(React.createElement(Harness));
    assert.deepEqual(root.errors, []);
  }
  await render();
  return {
    render,
    keyboard,
    views,
    frames,
    model: () => current!,
    mounts: () => ({ mounts, unmounts }),
    style: (id: string) => Object.assign({}, ...[views.get(id)!.style].flat()),
    flush: () =>
      root.act(() => {
        while (scheduled.length) scheduled.shift()!();
      }),
    act: root.act,
    close: root.close,
  };
}
