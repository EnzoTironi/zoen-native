import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createRequire } from 'node:module';
import { build } from 'esbuild';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import type { useMessageSelection } from '../components/use-message-selection';
import type { Dialogs } from '../components/dialog-types';
import type { useMessageActions } from '../components/use-message-actions';
import type { MessageActionsProps } from '../components/message-actions.types';
import type { MessageContextRequest } from '../components/message-context-ports';
import { bundled, message } from './message-author-navigation.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession, deferred } from './store.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
const require = createRequire(import.meta.url);
const selectionSource = new URL('../components/use-message-selection.ts', import.meta.url);
const selectionBundle = await build({
  stdin: {
    contents: compileExpoSource(selectionSource, true),
    resolveDir: dirname(fileURLToPath(selectionSource)),
    loader: 'js',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  external: [
    'react',
    'react/compiler-runtime',
    'react/jsx-runtime',
    'expo-router',
    'expo-clipboard',
    'react-native',
    '../core/provider',
    './dialogs',
    '../i18n/index.ts',
  ],
});

/** Real React, ClientStore and compiled action hook; native UI and commands are ports. */
export async function liveMessageActions() {
  const root = createReactTestRoot(),
    driver = new Driver();
  const snapshot = draftSession();
  const space = snapshot.conversations[0]!.id;
  let entry = message({
    author: snapshot.conversations[0]!.members[0]!,
    text_revision: 'signed-revision',
  });
  const second = message({
    id: 'synthetic_second',
    seq: '2',
    author: entry.author,
    text_revision: 'second-revision',
    kind: { Message: { text: 'Second message', card: null } },
  });
  let entries = [entry, second];
  let threadRoot: string | null = null;
  const confirmations: Parameters<Dialogs['choose']>[0][] = [];
  let choose: Dialogs['choose'] = async () => assert.fail('No deletion confirmation');
  let failMutation = false;
  const commands: unknown[] = [],
    routes: unknown[] = [],
    copies: string[] = [];
  const keyboardCalls: string[] = [],
    admissions: string[][] = [];
  let keyboard = Promise.resolve(),
    keyboardVisible = false,
    admission = Promise.resolve(true),
    row = true;
  const replies: unknown[] = [],
    edits: unknown[] = [],
    threads: unknown[] = [];
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'message')
      return entries.find((value) => value.id === command.message_id) ?? entry;
    if (command.op === 'set_typing') return undefined;
    commands.push(command);
    if (failMutation) throw Error('storage unavailable');
    if (command.op === 'delete_messages') {
      const removed = command.targets.map((target) => target.message_id);
      entries = entries.filter((value) => !removed.includes(value.id));
      return removed;
    }
    if (command.op === 'react') return entry;
    throw Error(`Unexpected message action ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  let focused = true,
    disabled = false,
    revision = 0;
  const Focus = React.createContext(true);
  const useClient = () => ({
    store,
    ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
  });
  const module = { exports: {} as { useMessageActions: typeof useMessageActions } };
  const ports = {
    module,
    exports: module.exports,
    Error,
    AbortController,
    require: (name: string) => {
      if (name === 'react') return React;
      if (name === 'react-native')
        return { Keyboard: { dismiss: () => keyboardCalls.push('selection-keyboard') } };
      if (
        name === 'react/compiler-runtime' ||
        name === 'react/jsx-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'expo-router')
        return {
          router: { push: (route: unknown) => routes.push(route) },
          useFocusEffect: (effect: React.EffectCallback) => {
            const isFocused = React.use(Focus);
            React.useEffect(() => (isFocused ? effect() : undefined), [effect, isFocused]);
          },
        };
      if (name === '../core/provider') return { useClient };
      if (name === 'expo-clipboard')
        return {
          setStringAsync: async (text: string) => {
            copies.push(text);
            return true;
          },
        };
      if (name === 'react-native-keyboard-controller')
        return {
          KeyboardController: {
            dismiss: () => {
              keyboardCalls.push('keyboard');
              return keyboard;
            },
            isVisible: () => keyboardVisible,
          },
        };
      if (name === '../media/share-message')
        return { shareMessageMedia: () => assert.fail('Text has no media export') };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === './dialogs')
        return {
          useDialogs: () => ({
            choose: (options: Parameters<Dialogs['choose']>[0]) => {
              confirmations.push(options);
              return choose(options);
            },
            inform: () => assert.fail('No error expected'),
          }),
        };
      if (name === './message-actions') return { MessageActions: () => null };
      if (name === './reaction-details') return { ReactionDetails: () => null };
      if (name === './message-info-navigation')
        return { messageInfoAction: () => assert.fail('No message-info navigation') };
      throw Error(name);
    },
  };
  runInNewContext(bundled.outputFiles[0]!.text, ports);
  const selectionModule = { exports: {} as { useMessageSelection: typeof useMessageSelection } };
  runInNewContext(selectionBundle.outputFiles[0]!.text, {
    ...ports,
    module: selectionModule,
    exports: selectionModule.exports,
  });
  let actions!: ReturnType<typeof useMessageActions>;
  let selected!: ReturnType<typeof useMessageSelection>;
  const onReply = (value: unknown) => {
    replies.push(value);
  };
  const onEdit = (value: unknown) => {
    edits.push(value);
  };
  const onThread = (value: unknown) => {
    threads.push(value);
  };
  function Probe(_props: { revision: number }) {
    selected = selectionModule.exports.useMessageSelection(space, entries, disabled, threadRoot);
    actions = module.exports.useMessageActions({
      spaceId: space,
      entries,
      disabled: disabled || selected.active,
      threadRootId: threadRoot,
      onReply,
      onEdit,
      onThread,
    });
    return null;
  }
  const render = () =>
    root.render(
      React.createElement(
        Focus.Provider,
        { value: focused },
        React.createElement(Probe, { revision: revision++ }),
      ),
    );
  await render();
  return {
    root,
    store,
    get entry() {
      return entry;
    },
    commands,
    confirmations,
    selection: () => selected,
    get entries() {
      return entries;
    },
    choose: (next: Dialogs['choose']) => {
      choose = next;
    },
    failMutation: (fail: boolean) => {
      failMutation = fail;
    },
    routes,
    copies,
    replies,
    edits,
    threads,
    keyboardCalls,
    admissions,
    holdKeyboard() {
      const gate = deferred<void>();
      keyboard = gate.promise;
      keyboardVisible = true;
      return () => {
        keyboardVisible = false;
        gate.resolve();
      };
    },
    holdAdmission() {
      const gate = deferred<boolean>();
      admission = gate.promise;
      return gate;
    },
    context(kind: 'hold' | 'double' = 'hold') {
      return {
        binding: 'row',
        gestureId: 'gesture',
        kind,
        current: () => row,
        native: {
          consumeGesture: async (binding: string, gesture: string) => {
            admissions.push([binding, gesture]);
            return admission;
          },
        },
      } as unknown as MessageContextRequest;
    },
    revokeRow() {
      row = false;
    },
    async replaceReaction() {
      entry = { ...entry, reactions: [{ emoji: '❤️', mine: true, people: [entry.author] }] };
      entries = entries.map((value) => (value.id === entry.id ? entry : value));
      await render();
    },
    async startContext(request: MessageContextRequest, mode: 'actions' | 'reactions' = 'actions') {
      let done!: Promise<void>;
      await root.act(() => {
        done = actions.open(entry, mode, undefined, undefined, undefined, request);
      });
      return { done };
    },
    panel: () => (actions.panel as React.ReactElement<MessageActionsProps>).props,
    async open() {
      await root.act(() => actions.open(entry, 'actions'));
    },
    async change(
      kind:
        | 'reopen'
        | 'relay'
        | 'access-aba'
        | 'focus-aba'
        | 'disabled'
        | 'owner'
        | 'access'
        | 'blur'
        | 'thread-aba',
    ) {
      if (kind === 'thread-aba') {
        const previous = entries;
        entries = [];
        threadRoot = 'another-thread';
        await render();
        entries = previous;
        threadRoot = null;
        await render();
      }
      if (kind === 'owner')
        await root.act(async () => {
          snapshot.account!.identity_id = 'c'.repeat(64);
          await store.open('account');
          assert.equal(
            store.getSnapshot().snapshot?.account?.identity_id,
            snapshot.account!.identity_id,
          );
        });
      if (kind === 'access')
        await root.act(async () => {
          snapshot.conversations = [];
          await store.refresh();
          assert.deepEqual(store.getSnapshot().snapshot?.conversations, []);
        });
      if (kind === 'blur') {
        focused = false;
        await render();
      }
      if (kind === 'reopen') await root.act(() => store.open('account'));
      if (kind === 'relay')
        await root.act(async () => {
          snapshot.account!.relay_url = 'https://another-relay.example';
          await store.refresh();
          assert.equal(
            store.getSnapshot().snapshot?.account?.relay_url,
            snapshot.account!.relay_url,
          );
        });
      if (kind === 'access-aba')
        await root.act(async () => {
          const chats = snapshot.conversations;
          snapshot.conversations = [];
          await store.refresh();
          assert.deepEqual(store.getSnapshot().snapshot?.conversations, []);
          snapshot.conversations = chats;
          await store.refresh();
        });
      if (kind === 'focus-aba') {
        focused = false;
        await render();
        focused = true;
        await render();
      }
      if (kind === 'disabled') {
        disabled = true;
        await render();
        disabled = false;
        await render();
      }
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
