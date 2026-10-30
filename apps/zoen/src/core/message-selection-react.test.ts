import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import { hookHost } from './message-selection-react.fixtures.ts';
import { draftSession } from './store.fixtures.ts';
import * as selection from './message-selection.ts';
import * as deletion from './message-deletion.ts';
import { conversationScope } from './conversation-scope.ts';
import type { TimelineEntry } from './types';
import type { DialogOptions } from '../components/dialog-types';
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
  const listeners = new Set<() => void>();
  const commands: unknown[] = [];
  const confirmations: DialogOptions[] = [];
  let answer: (value: boolean) => void = () => {};
  let fail = false;
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    setTyping: () => {},
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    request: async (command: unknown) => {
      commands.push(JSON.parse(JSON.stringify(command)));
      if (fail) throw new Error('storage unavailable');
      return ['a', 'b'];
    },
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
      if (name === '../core/message-deletion') return deletion;
      if (name === './dialogs')
        return {
          useDialogs: () => ({
            confirm: (options: DialogOptions) => {
              confirmations.push(options);
              return new Promise<boolean>((resolve) => {
                answer = resolve;
              });
            },
          }),
        };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === '../core/provider') return { useClient: () => ({ ...state, store }) };
      throw new Error(name);
    },
  });
  let entries = [
    { id: 'a', kind: { Message: { text: 'first', card: null } }, deleted: false, voice: null },
    { id: 'b', kind: { Message: { text: 'second', card: null } }, deleted: false, voice: null },
  ].map((entry) => ({
    ...entry,
    text_revision: `revision-${entry.id}`,
    author: { id: state.snapshot.account!.identity_id, is_me: true },
  })) as TimelineEntry[];
  let disabled = false;
  let threadRoot: string | null = null;
  return {
    copied,
    opened,
    commands,
    confirmations,
    answer: (value: boolean) => answer(value),
    fail: () => {
      fail = true;
    },
    recover: () => {
      fail = false;
    },
    notify: () => {
      for (const listener of listeners) listener();
    },
    listeners,
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
    readOnly: () => {
      state = {
        ...state,
        snapshot: {
          ...state.snapshot,
          conversations: state.snapshot.conversations.map((chat) => ({
            ...chat,
            can_write: false,
          })),
        },
      };
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

const settleDeletion = () => new Promise<void>((resolve) => setImmediate(resolve));
test('real hook confirms the exact owned set once and clears only after successful deletion', async () => {
  const f = fixture();
  f.render().toggle(f.entries[1]!);
  f.render().toggle(f.entries[0]!);
  const selected = f.render();
  assert.equal(selected.canDelete, true);
  selected.remove();
  selected.remove();
  assert.equal(f.confirmations.length, 1);
  assert.equal(f.confirmations[0]!.destructive, true);
  assert.equal(f.render().pending, true);
  assert.deepEqual(f.commands, []);
  f.answer(true);
  await settleDeletion();
  assert.deepEqual(f.commands, [
    {
      op: 'delete_messages',
      expected_identity_id: draftSession().account!.identity_id,
      expected_device_id: draftSession().account!.device_id,
      space_id: draftSession().conversations[0]!.id,
      targets: [
        { message_id: 'a', expected_revision: 'revision-a' },
        { message_id: 'b', expected_revision: 'revision-b' },
      ],
    },
  ]);
  assert.equal(f.render().count, 0);
  assert.equal(f.render().pending, false);
  assert.equal(f.listeners.size, 0);
});

test('cancel and command failure preserve selection for a fresh confirmation and retry', async () => {
  const f = fixture();
  f.render().toggle(f.entries[0]!);
  f.render().remove();
  f.answer(false);
  await settleDeletion();
  assert.equal(f.render().count, 1);
  assert.equal(f.commands.length, 0);
  f.fail();
  f.render().remove();
  f.answer(true);
  await settleDeletion();
  assert.equal(f.render().count, 1);
  assert.equal(f.render().pending, false);
  f.recover();
  f.render().remove();
  f.answer(true);
  await settleDeletion();
  assert.equal(f.confirmations.length, 3);
  assert.equal(f.commands.length, 2);
  assert.equal(f.render().count, 0);
  assert.equal(f.listeners.size, 0);
});

for (const change of [
  'blur',
  'account',
  'epoch',
  'retiring',
  'access',
  'reader',
  'selection',
  'revision',
  'cancel',
] as const)
  test(`a pending real native confirmation cannot delete after ${change}`, async () => {
    const f = fixture();
    f.render().toggle(f.entries[0]!);
    const selected = f.render();
    selected.remove();
    if (change === 'blur') f.blur();
    if (change === 'account') f.switchAccount();
    if (change === 'epoch') f.store.sessionEpoch++;
    if (change === 'retiring') f.store.isSessionRetiring = true;
    if (change === 'access') f.revoke();
    if (change === 'reader') f.readOnly();
    if (change === 'selection') selected.toggle(f.entries[1]!);
    if (change === 'revision') f.entries[0]!.text_revision = 'edited';
    if (change === 'cancel') selected.cancel();
    f.render();
    f.notify();
    f.answer(true);
    await settleDeletion();
    assert.deepEqual(f.commands, []);
    assert.equal(f.listeners.size, 0);
    assert.equal(f.render().pending, false);
  });
