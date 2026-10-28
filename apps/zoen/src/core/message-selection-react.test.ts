import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import { hookHost } from './message-selection-react.fixtures.ts';
import { draftSession } from './store.fixtures.ts';
import * as selection from './message-selection.ts';
import { conversationScope } from './conversation-scope.ts';
import type { TimelineEntry } from './types';
import type { useMessageSelection } from '../components/use-message-selection';

const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/use-message-selection.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  bundle: true,
  external: ['*'],
  platform: 'node',
  format: 'cjs',
  write: false,
});
function fixture() {
  const host = hookHost();
  let state = {
    phase: 'ready' as const,
    mode: 'account' as const,
    erasePending: false,
    snapshot: draftSession(),
  };
  const space = state.snapshot.conversations[0]!.id;
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    setTyping: () => {},
  };
  const copied: string[] = [];
  const opened: unknown[] = [];
  const module = {
    exports: {} as {
      useMessageSelection: typeof useMessageSelection;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return host.react;
      if (name === 'expo-router')
        return {
          router: { push: (value: unknown) => opened.push(value) },
          useFocusEffect: host.useFocusEffect,
        };
      if (name === 'react-native') return { Keyboard: { dismiss() {} } };
      if (name === 'expo-clipboard')
        return {
          setStringAsync: async (value: string) => {
            copied.push(value);
          },
        };
      if (name === '../core/conversation-scope') return { conversationScope };
      if (name === '../core/message-selection') return selection;
      if (name === '../core/provider') return { useClient: () => ({ ...state, store }) };
      throw new Error(name);
    },
  });
  let entries = [
    { id: 'a', kind: { Message: { text: 'first', card: null } }, deleted: false, voice: null },
    { id: 'b', kind: { Message: { text: 'second', card: null } }, deleted: false, voice: null },
  ] as TimelineEntry[];
  let disabled = false;
  let threadRoot: string | null = null;
  return {
    copied,
    opened,
    store,
    entries,
    render: () =>
      host.render(() => module.exports.useMessageSelection(space, entries, disabled, threadRoot)),
    setThread: (root: string | null) => {
      threadRoot = root;
    },
    blur: host.blur,
    revoke: () => {
      state = { ...state, snapshot: { ...state.snapshot, conversations: [] } };
    },
    switchAccount: () => {
      state = {
        ...state,
        snapshot: {
          ...state.snapshot,
          account: { ...state.snapshot.account!, identity_id: 'other' },
        },
      };
    },
    remove: () => {
      entries = [];
    },
    disable: () => {
      disabled = true;
    },
  };
}
test('real selection hook toggles without navigation or mutation and forwards ordered identities only', () => {
  const f = fixture();
  f.render().toggle(f.entries[1]!);
  f.render().toggle(f.entries[0]!);
  const selected = f.render();
  assert.equal(selected.count, 2);
  assert.equal(f.opened.length, 0);
  selected.copy();
  assert.deepEqual(f.copied, ['first\n\nsecond']);
  selected.forward();
  const route = f.opened[0] as {
    pathname: string;
    params: { messages: string; selectionScope: string };
  };
  assert.equal(route.pathname, '/forward-selection/[space]');
  assert.equal(route.params.messages, '["a","b"]');
  assert.ok(route.params.selectionScope);
});
for (const reason of ['blur', 'access', 'account', 'epoch', 'retiring'] as const)
  test(`retained real selection callbacks cannot copy or open forwarding after ${reason}`, () => {
    const f = fixture();
    f.render().toggle(f.entries[0]!);
    const retained = f.render();
    if (reason === 'blur') f.blur();
    if (reason === 'access') f.revoke();
    if (reason === 'account') f.switchAccount();
    if (reason === 'epoch') f.store.sessionEpoch++;
    if (reason === 'retiring') f.store.isSessionRetiring = true;
    retained.copy();
    retained.forward();
    retained.toggle(f.entries[1]!);
    assert.deepEqual(f.copied, []);
    assert.deepEqual(f.opened, []);
    if (reason !== 'retiring') assert.equal(f.render().count, 0);
  });
test('real hook retires removed selections and search mode without reviving on later renders', () => {
  const f = fixture();
  f.render().toggle(f.entries[0]!);
  f.remove();
  assert.equal(f.render().count, 0);
  const g = fixture();
  g.render().toggle(g.entries[0]!);
  g.disable();
  assert.equal(g.render().count, 0);
});

test('retained copy and forward callbacks cannot reuse a changed selected set after rerender', () => {
  const f = fixture();
  f.render().toggle(f.entries[0]!);
  const retained = f.render();
  retained.toggle(f.entries[1]!);
  const current = f.render();
  assert.equal(current.count, 2);
  retained.copy();
  retained.forward();
  assert.deepEqual(f.copied, []);
  assert.deepEqual(f.opened, []);
  current.copy();
  current.forward();
  assert.deepEqual(f.copied, ['first\n\nsecond']);
  assert.equal((f.opened[0] as { params: { messages: string } }).params.messages, '["a","b"]');
});

test('real selection authority retires when reused between conversation and distinct thread roots', () => {
  const f = fixture();
  f.setThread('first-thread');
  f.render().toggle(f.entries[0]!);
  const previousThread = f.render();
  f.setThread('second-thread');
  assert.equal(f.render().count, 0);
  previousThread.copy();
  previousThread.forward();
  previousThread.toggle(f.entries[1]!);
  assert.deepEqual(f.copied, []);
  assert.deepEqual(f.opened, []);
  assert.equal(f.render().count, 0);
  f.render().toggle(f.entries[1]!);
  assert.equal(f.render().count, 1);
  f.setThread(null);
  assert.equal(f.render().count, 0);
});
