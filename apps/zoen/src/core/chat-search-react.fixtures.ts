import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { chatNativeMetricsModule } from './chat-native-metrics.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession, deferred } from './store.fixtures.ts';
import * as conversationSearch from './conversation-search.ts';
import * as conversationReveal from './conversation-reveal.ts';
import * as conversationContent from './conversation-reveal-content.ts';
import * as conversationTimeline from './conversation-timeline.ts';
import * as initialEnd from './initial-timeline-end.ts';
import * as activity from './live-activity.ts';
import * as conversationScope from './conversation-scope.ts';
import * as messageSelection from './message-selection.ts';
import * as messageDeletion from './message-deletion.ts';
import type { useChatController } from '../components/use-chat-controller.ts';
import type { TimelinePage } from './generated/TimelinePage';
import type { MessageSearchPage } from './generated/MessageSearchPage';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('../components/use-chat-controller.ts', import.meta.url);
const compiled = transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.ok(compiled.includes('useChatController'), 'Expo must transform the actual chat controller');

/** Actual Expo-transformed chat controller and search; geometry/native views are host ports. */
export async function chatSearchReactFixture(qa = false) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const id = snapshot.conversations[0]!.id;
  const calls: any[] = [];
  const restoreRecords: string[] = [];
  const pending = deferred<MessageSearchPage>();
  let timeline = async (): Promise<TimelinePage> => ({
    entries: [],
    has_more: false,
    before: null,
  });
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'timeline') return timeline();
    if (command.op === 'mark_read') return null;
    if (command.op === 'search_messages') return pending.promise;
    if (command.op === 'message') throw Error('Synthetic search result has no loaded message');
    throw Error(`Unexpected search operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const navigation = { isFocused: () => true, addListener: () => () => {} };
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
  const normalDraft = {
    messageDraft: { text: '', reply: null, editing: null, editText: '' },
    updateDraft() {},
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'expo-router': {
      useFocusEffect: focus.exports.useFocusEffect,
      useIsFocused: () => true,
      useLocalSearchParams: () => ({ id }),
    },
    'expo-router/react-navigation': { useHeaderHeight: () => 64 },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ top: 20 }) },
    '../components/dialogs': { useDialogs: () => ({ inform: async () => {} }) },
    '../components/keyboard-motion': {
      useKeyboardMotion: () => ({ progress: { value: 0 }, height: { value: 0 } }),
    },
    '../components/use-message-actions': { useMessageActions: () => ({}) },
    './use-chat-native-metrics': chatNativeMetricsModule({
      qa,
      native: qa,
      emit: (line) => restoreRecords.push(line),
      appState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
      useFocusEffect: focus.exports.useFocusEffect as (callback: () => void | (() => void)) => void,
    }),
    './use-conversation-rail': { useConversationRail: () => ({ visibleHeight: { value: 0 } }) },
    '../core/conversation-reveal': conversationReveal,
    '../core/conversation-reveal-content': conversationContent,
    '../core/conversation-search': conversationSearch,
    '../core/conversation-timeline': conversationTimeline,
    '../core/initial-timeline-end': initialEnd,
    '../core/live-activity': activity,
    '../core/conversation-scope': conversationScope,
    '../core/message-presentation': { messageCard: () => null },
    '../core/presentation': { messageText: () => '' },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../core/use-message-draft': { useMessageDraft: () => normalDraft },
    '../home/provider': { useHome: () => ({ allItems: [] }) },
    '../i18n/index.ts': { t: (s: string) => s },
    '../media/message-audio-provider': { useMessageAudioFocus() {} },
    '../media/mosaic-layout': { supportsMediaMosaic: () => false },
    '../miniapps/conversation-provider': {
      useConversationCards: () => ({ visible: [], loading: false }),
    },
    '../theme': { useTheme: () => ({ colors: {} }) },
  };
  const hookFile = new URL('./use-conversation-search.ts', import.meta.url);
  const hookCode = transformSync(readFileSync(hookFile, 'utf8'), {
    filename: hookFile.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
  assert.ok(hookCode.includes('react/compiler-runtime'), 'production search hook must be compiled');
  const hook = { exports: {} };
  runInNewContext(hookCode, {
    module: hook,
    exports: hook.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === './provider') return ports['../core/provider'];
      if (name === './conversation-scope') return conversationScope;
      if (name === './conversation-search') return conversationSearch;
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  ports['../core/use-conversation-search'] = hook.exports;
  const replyFocusFile = new URL('./use-reply-destination-focus.ts', import.meta.url);
  const replyFocusCode = transformSync(readFileSync(replyFocusFile, 'utf8'), {
    filename: replyFocusFile.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
  const replyFocus = { exports: {} };
  runInNewContext(replyFocusCode, {
    module: replyFocus,
    exports: replyFocus.exports,
    requestAnimationFrame: () => 1,
    cancelAnimationFrame() {},
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  ports['../core/use-reply-destination-focus'] = replyFocus.exports;
  const selectionFile = new URL('../components/use-message-selection.ts', import.meta.url);
  const selectionCode = transformSync(readFileSync(selectionFile, 'utf8'), {
    filename: selectionFile.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
  const selection = { exports: {} };
  runInNewContext(selectionCode, {
    module: selection,
    exports: selection.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react-native') return { Keyboard: { dismiss() {} } };
      if (name === 'expo-clipboard') return { setStringAsync: async () => {} };
      if (name === '../core/message-selection') return messageSelection;
      if (name === '../core/message-deletion') return messageDeletion;
      if (name === './dialogs') return { useDialogs: () => ({ confirm: async () => false }) };
      if (name === '../i18n') return ports['../i18n/index.ts'];
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  ports['./use-message-selection'] = selection.exports;

  const module = { exports: {} as { useChatController: typeof useChatController } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    setTimeout,
    clearTimeout,
    process: { env: { EXPO_OS: 'ios' } },
    requestAnimationFrame: () => 1,
    cancelAnimationFrame() {},
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let current: ReturnType<typeof useChatController> | null = null;
  function Surface() {
    const value = module.exports.useChatController();
    React.useLayoutEffect(() => {
      current = value;
    }, [value]);
    return null;
  }
  return {
    store,
    snapshot,
    calls,
    restoreRecords,
    pending,
    errors: root.errors,
    act: root.act,
    current() {
      assert.ok(current);
      return current;
    },
    timeline: (read: typeof timeline) => {
      timeline = read;
    },
    render: () => root.render(React.createElement(Surface)),
    async reopen(change?: 'relay') {
      await root.act(async () => {
        await store.close();
        if (change === 'relay')
          snapshot.account = { ...snapshot.account!, relay_url: 'https://other.invalid' };
        await store.open('account');
      });
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
