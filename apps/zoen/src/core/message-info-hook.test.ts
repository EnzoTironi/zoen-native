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
const compiled = require('@babel/core').transformSync(
  readFileSync(process.env.ZOEN_MESSAGE_INFO_SOURCE ?? file, 'utf8'),
  {
    filename: file.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  },
).code as string;
assert.match(compiled, /react\/compiler-runtime/);

async function fixture(initialForeground = 'active') {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  snapshot.connection = { state: 'online', synced: true, pending: '0', error: null };
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
  let foreground = initialForeground;
  const listeners = new Set<(next: string) => void>();
  const ports: Record<string, unknown> = {
    'react-native': {
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener(_type: string, listener: (next: string) => void) {
          listeners.add(listener);
          return { remove: () => listeners.delete(listener) };
        },
      },
    },
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
  const committed: { target: string; value: MessageInfo | null }[] = [];
  function Surface({ target = 'message' }: { target?: string }) {
    const value = module.exports.useMessageInfo(spaceId, target, expected);
    React.useLayoutEffect(() => {
      current = value;
      committed.push({ target, value: value.value });
    }, [target, value]);
    return null;
  }
  return {
    root,
    driver,
    snapshot,
    store,
    reads,
    calls,
    spaceId,
    committed,
    current: () => current,
    render: (target = 'message') => root.render(React.createElement(Surface, { target })),
    captureAppState: () => [...listeners].at(-1),
    foreground: (next: string) =>
      root.act(() => {
        foreground = next;
        for (const listener of listeners) listener(next);
      }),
    async close() {
      await root.close();
      await store.close();
    },
  };
}

test('actual Expo Info ignores typing/other chats and retires canonical, membership and account changes', async () => {
  const f = await fixture();
  const { root, driver, snapshot, store, reads, calls, spaceId, committed } = f;
  try {
    await f.render();
    assert.equal(reads.length, 1);
    const info: MessageInfo = {
      scope: calls.find((call) => call.op === 'message_info').scope,
      author: snapshot.conversations[0]!.members[0]!,
      sent_at_ms: '1000',
      forwarded: false,
      edited: false,
      edited_at_ms: null,
      delivery: 'Sent',
      read_details: 'available',
      participants: [],
    };
    await root.act(async () => {
      reads[0]!.resolve(info);
    });
    assert.equal(f.current()!.value!.sent_at_ms, '1000');
    await f.render('next');
    assert.equal(reads.length, 2);
    assert.equal(committed.find((entry) => entry.target === 'next')?.value, null);
    const nextInfo = {
      ...info,
      scope: calls.filter((call) => call.op === 'message_info')[1].scope,
    };
    await root.act(async () => {
      reads[1]!.resolve(nextInfo);
    });
    assert.equal(f.current()!.value?.scope.message_id, 'next');
    const beforeActivity = store.getSnapshot();
    await root.act(async () => {
      driver.changed!({
        kind: 'ephemeral',
        spaceId,
        fromId: snapshot.conversations[0]!.members[0]!.id,
        type: 'typing',
        detail: '',
      });
    });
    assert.notEqual(
      store.getSnapshot(),
      beforeActivity,
      'actual activity emits a store publication',
    );
    assert.equal(reads.length, 2, 'typing does not reread metadata');
    assert.equal(f.current()!.value!.sent_at_ms, '1000');
    await root.act(() => store.refresh(['other-chat']));
    assert.equal(reads.length, 2, 'another conversation does not invalidate Info');
    assert.equal(f.current()!.value!.sent_at_ms, '1000');
    await root.act(() => store.refresh([spaceId]));
    assert.equal(reads.length, 3, 'canonical changes to this conversation reread');
    assert.equal(
      f.current()!.value,
      null,
      'invalidated canonical data is not retained optimistically',
    );
    const members = snapshot.conversations[0]!.members;
    snapshot.conversations[0]!.members = [];
    await root.act(() => store.refresh(['other-chat']));
    assert.equal(f.current()!.value, null, 'membership loss retires immediately on publication');
    snapshot.conversations[0]!.members = members;
    await root.act(() => store.refresh(['other-chat']));
    await root.act(async () => {
      reads[2]!.resolve(nextInfo);
    });
    assert.equal(f.current()!.value, null, 'membership ABA cannot revive a held result');
    await root.act(() => store.refresh([spaceId]));
    assert.equal(reads.length, 4);
    await root.act(async () => {
      await store.close();
      await store.open('account');
      reads[3]!.resolve(nextInfo);
    });
    assert.equal(f.current()!.value, null);
    assert.equal(reads.length, 4);
    assert.ok(calls.every((call) => call.op === 'message_info' || call.op === 'snapshot'));
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
    await store.close();
  }
});

test('Message Info clears on background, rejects late metadata and refreshes current evidence on foreground', async () => {
  const f = await fixture();
  const info = (): MessageInfo => ({
    scope: f.calls.filter((call) => call.op === 'message_info').at(-1).scope,
    author: f.snapshot.conversations[0]!.members[0]!,
    sent_at_ms: '1000',
    forwarded: false,
    edited: false,
    edited_at_ms: null,
    delivery: 'Read',
    read_details: 'available',
    participants: [],
  });
  try {
    await f.render();
    await f.root.act(() => f.reads[0]!.resolve(info()));
    assert.equal(f.current()!.value?.delivery, 'Read');
    await f.root.act(() => f.store.refresh([f.spaceId]));
    assert.equal(f.reads.length, 2);
    const old = info();
    await f.foreground('background');
    assert.equal(f.current()!.value, null);
    await f.root.act(() => f.reads[1]!.resolve(old));
    await f.root.act(() => f.store.refresh([f.spaceId]));
    assert.equal(f.reads.length, 2, 'background must not reread private participant details');
    assert.equal(f.current()!.value, null);
    await f.foreground('active');
    assert.equal(f.reads.length, 3);
    await f.root.act(() => f.reads[2]!.resolve(info()));
    assert.equal(f.current()!.value?.delivery, 'Read');
    await f.foreground('active');
    assert.equal(f.reads.length, 3, 'duplicate active notices do not reload');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('Message Info removes visible details on background and cannot borrow a reopened account', async () => {
  const f = await fixture();
  try {
    await f.render();
    await f.root.act(() =>
      f.reads[0]!.resolve({
        scope: f.calls[0].scope,
        author: f.snapshot.conversations[0]!.members[0]!,
        sent_at_ms: '1000',
        forwarded: false,
        edited: false,
        edited_at_ms: null,
        delivery: 'Read',
        read_details: 'available',
        participants: [],
      }),
    );
    assert.ok(f.current()!.value);
    await f.foreground('inactive');
    assert.equal(f.current()!.value, null);
    await f.root.act(async () => {
      await f.store.close();
      await f.store.open('account');
    });
    await f.foreground('active');
    assert.equal(f.reads.length, 1);
    assert.equal(f.current()!.value, null);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('a background Info route waits for foreground before requesting private metadata', async () => {
  const f = await fixture('background');
  try {
    await f.render();
    assert.equal(f.reads.length, 0);
    await f.root.act(() => f.store.refresh([f.spaceId]));
    assert.equal(f.reads.length, 0);
    await f.foreground('active');
    assert.equal(f.reads.length, 1);
    assert.equal(f.current()!.loading, true);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('a held AppState callback cannot refocus an unmounted Message Info route', async () => {
  const f = await fixture();
  try {
    await f.render();
    const held = f.captureAppState();
    assert.ok(held);
    await f.root.render(null);
    await f.root.act(() => {
      held('background');
      held('active');
    });
    assert.equal(f.reads.length, 1, 'retired native callbacks must not start a new private query');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});
