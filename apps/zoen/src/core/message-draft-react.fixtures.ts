import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import * as draft from './message-draft.ts';
import * as i18n from '../i18n/index.ts';
import * as presentation from './message-presentation.ts';
import type { useMessageDraft } from './use-message-draft.ts';
import type { MessageDraftEdit } from './generated/MessageDraftEdit';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('./use-message-draft.ts', import.meta.url);
const compiled = transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.match(compiled, /react\/compiler-runtime/);

/** Real compiled hook and durable draft service with synthetic scoped Core IO. */
export async function messageDraftReactFixture(open = true) {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const spaceId = snapshot.conversations[0]!.id;
  const calls: any[] = [];
  let text = 'Owner A private unsent text';
  let revision = 1;
  let edit: MessageDraftEdit | undefined;
  const draftValue = () => ({
    revision: String(revision),
    text,
    reply_to: null,
    ...(edit ? { edit } : {}),
  });
  driver.durableMessageDrafts = true;
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'message_draft') return draftValue();
    if (command.op === 'save_message_draft') {
      assert.equal(command.expected_identity_id, snapshot.account!.identity_id);
      assert.equal(command.expected_device_id, snapshot.account!.device_id);
      assert.equal(command.expected_revision, String(revision));
      text = command.text;
      edit = command.edit;
      ++revision;
      return draftValue();
    }
    if (command.op === 'send_message_edit_draft') {
      assert.equal(command.expected_revision, String(revision));
      assert.ok(edit);
      const entry = { id: edit.message_id, text_revision: 'accepted-source-revision' };
      edit = undefined;
      ++revision;
      return {
        message_id: entry.id,
        text_revision: entry.text_revision ?? null,
        entry,
        draft: draftValue(),
      };
    }
    throw Error(`Unexpected draft operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  if (open) await store.open('account');
  const listeners = new Set<(next: string) => void>();
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
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      AppState: {
        addEventListener(_: string, callback: (next: string) => void) {
          listeners.add(callback);
          return { remove: () => listeners.delete(callback) };
        },
      },
    },
    'expo-router': { useFocusEffect: focus.exports.useFocusEffect },
    './provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    './message-draft': draft,
    './message-presentation': presentation,
    '../i18n/index.ts': i18n,
  };
  const module = { exports: {} as { useMessageDraft: typeof useMessageDraft } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let current: ReturnType<typeof useMessageDraft> | null = null;
  function Surface() {
    const value = module.exports.useMessageDraft(spaceId);
    React.useLayoutEffect(() => {
      current = value;
    }, [value]);
    return null;
  }
  return {
    driver,
    store,
    snapshot,
    calls,
    errors: root.errors,
    act: root.act,
    current() {
      assert.ok(current);
      return current;
    },
    render: () => root.render(React.createElement(Surface)),
    unmount: () => root.render(null),
    open: () => root.act(() => store.open('account')),
    async reopen(change?: 'identity' | 'device' | 'relay', preserve = false) {
      await root.act(async () => {
        await store.close();
        if (change === 'identity') {
          snapshot.account = { ...snapshot.account!, identity_id: 'e'.repeat(64) };
          snapshot.conversations[0]!.members[0]!.id = snapshot.account.identity_id;
        }
        if (change === 'device')
          snapshot.account = { ...snapshot.account!, device_id: 'f'.repeat(64) };
        if (change === 'relay')
          snapshot.account = { ...snapshot.account!, relay_url: 'https://other-relay.invalid/' };
        if (!preserve) {
          text = 'New session canonical draft';
          revision = 1;
          edit = undefined;
        }
        await store.open('account');
      });
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
