import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, deferred, draftSession } from './store.fixtures.ts';
import * as conversationScope from './conversation-scope.ts';
import * as accountScope from './account-scope.ts';
import { t } from '../i18n/index.ts';
import type { Snapshot, TimelineEntry } from './types.ts';
import type { useSemanticMessagePreview } from '../components/use-semantic-message-preview.ts';

const require = createRequire(import.meta.url);
const file = new URL('../components/use-semantic-message-preview.ts', import.meta.url);
const compiled = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.match(compiled, /react\/compiler-runtime/);

type Source = Parameters<typeof useSemanticMessagePreview>[0];
export async function semanticPreviewFixture(initial: Source) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => structuredClone(snapshot);
  let entries: TimelineEntry[] = [];
  const reads: { command: unknown; result: ReturnType<typeof deferred<TimelineEntry>> }[] = [];
  const snapshotReads: ReturnType<typeof deferred<Snapshot>>[] = [];
  driver.operation = async (command) => {
    if (command.op === 'snapshot') {
      const result = deferred<Snapshot>();
      snapshotReads.push(result);
      return result.promise;
    }
    if (command.op === 'timeline') return { entries, before: null, has_more: false };
    if (command.op === 'message') {
      const result = deferred<TimelineEntry>();
      reads.push({ command: structuredClone(command), result });
      return result.promise;
    }
    throw Error(`Unexpected operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  let focused = true;
  const focusListeners = new Map<string, Set<() => void>>();
  const navigation = {
    isFocused: () => focused,
    addListener(name: string, listener: () => void) {
      if (!focusListeners.has(name)) focusListeners.set(name, new Set());
      focusListeners.get(name)!.add(listener);
      return () => focusListeners.get(name)!.delete(listener);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as { useFocusEffect: unknown } };
  runInNewContext(
    readFileSync(require.resolve('expo-router/build/link/useLoadedNavigation.js'), 'utf8'),
    {
      module: loaded,
      exports: loaded.exports,
      require(name: string) {
        if (name === 'react') return React;
        if (name === '../react-navigation/native')
          return {
            useNavigation: () => navigation,
            NavigationContainerRefContext: React.createContext({}),
          };
        throw Error(name);
      },
    },
  );
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const projections: { markdown: string; result: ReturnType<typeof deferred<string>> }[] = [];
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'expo-router': { useFocusEffect: focus.exports.useFocusEffect },
    'react-native-enriched-markdown': {
      markdownToPlainText(markdown: string) {
        const result = deferred<string>();
        projections.push({ markdown, result });
        return result.promise;
      },
    },
    '../core/account-scope': accountScope,
    '../core/conversation-scope': conversationScope,
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../i18n/index.ts': { t },
  };
  const module = { exports: {} as { useSemanticMessagePreview: typeof useSemanticMessagePreview } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let source = initial,
    current!: ReturnType<typeof useSemanticMessagePreview>;
  function Preview() {
    const value = module.exports.useSemanticMessagePreview(source);
    React.useLayoutEffect(() => {
      current = value;
    }, [value]);
    return null;
  }
  const render = async (next = source) => {
    source = next;
    await root.render(React.createElement(Preview));
  };
  return {
    store,
    snapshot,
    reads,
    snapshotReads,
    projections,
    act: root.act,
    errors: root.errors,
    render,
    current: () => current,
    async refresh(spaces?: readonly string[]) {
      let done!: Promise<void>;
      await root.act(() => {
        done = store.refresh(spaces);
      });
      return { done };
    },
    async page(next: TimelineEntry[]) {
      entries = next;
      await root.act(() => store.loadTimeline(initial.target?.spaceId ?? 'chat'));
    },
    async focus(value: boolean) {
      focused = value;
      await root.act(() => {
        for (const listener of focusListeners.get(value ? 'focus' : 'blur') ?? []) listener();
      });
    },
    async reopen() {
      await root.act(async () => {
        await store.close();
        await store.open('account');
      });
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
