import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession, deferred } from './store.fixtures.ts';
import * as session from './message-info-session.ts';
import * as scope from './conversation-scope.ts';
import * as access from '../home/access.ts';
import type { useMessageInfo } from '../components/use-message-info';
import type { MessageInfo } from './generated/MessageInfo';

const require = createRequire(import.meta.url);
const file = new URL('../components/use-message-info.ts', import.meta.url);
const compiled = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
assert.match(compiled, /react\/compiler-runtime/);

test('actual Expo compiled Info hook retires held reads on account lifetime change and only requests metadata', async () => {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const calls: any[] = [],
    reads: ReturnType<typeof deferred<MessageInfo>>[] = [];
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(command);
    if (command.op === 'message_info') {
      const read = deferred<MessageInfo>();
      reads.push(read);
      return read.promise;
    }
    if (command.op === 'snapshot') return structuredClone(snapshot);
    throw Error(`Unexpected Info operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const spaceId = snapshot.conversations[0]!.id;
  const expected = scope.conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId)!;
  const navigation = { isFocused: () => true, addListener: () => () => {} };
  const focus = { exports: {} as { useFocusEffect: unknown } };
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return { useOptionalNavigation: () => navigation };
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'expo-router': focus.exports,
    '../core/provider': { useClient: () => ({ store }) },
    '../core/message-info-session': session,
    '../core/conversation-scope': scope,
    '../home/access': access,
  };
  const module = { exports: {} as { useMessageInfo: typeof useMessageInfo } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let current: ReturnType<typeof useMessageInfo> | undefined;
  function Surface() {
    const value = module.exports.useMessageInfo(spaceId, 'message', expected);
    React.useLayoutEffect(() => {
      current = value;
    }, [value]);
    return null;
  }
  try {
    await root.render(React.createElement(Surface));
    assert.equal(reads.length, 1);
    const info: MessageInfo = {
      scope: calls.find((call) => call.op === 'message_info').scope,
      author: snapshot.conversations[0]!.members[0]!,
      sent_at_ms: '1000',
      edited: false,
      edited_at_ms: null,
      delivery: 'Sent',
      read_details: 'available',
      participants: [],
    };
    await root.act(async () => {
      await store.close();
      await store.open('account');
      reads[0]!.resolve(info);
    });
    assert.equal(current!.value, null);
    assert.equal(reads.length, 1);
    assert.ok(calls.every((call) => call.op === 'message_info' || call.op === 'snapshot'));
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
    await store.close();
  }
});
