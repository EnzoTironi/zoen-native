import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { NormalMessageDraftService } from './normal-message-draft-service.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import type { ClientState, ClientStore } from './store.ts';
import type { Command, Snapshot } from './types.ts';

type Element = { type: unknown; props: Record<string, unknown> };
type Draft = { revision: string; text: string; reply_to: string | null };
const source = fileURLToPath(
  new URL('../components/conversation-composer-access.tsx', import.meta.url),
);
const ports = new Set([
  'react/jsx-runtime',
  'react-native',
  'react-native-safe-area-context',
  '../core/provider',
  '../theme',
  '../i18n/index.ts',
]);
const compiled = await build({
  stdin: { contents: readFileSync(source, 'utf8'), resolveDir: dirname(source), loader: 'tsx' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'composer-access-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function fixture() {
  let state: ClientState = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      me: { id: 'me' },
      account: { identity_id: 'me', device_id: 'phone', unlocked: true, setup_pending: false },
      conversations: [{ id: 'retained-chat', members: [{ id: 'me' }, { id: 'peer' }] }],
    } as unknown as Snapshot,
    timelines: {},
    activity: {} as ClientState['activity'],
    notice: null,
  };
  let load = async (): Promise<Draft> => ({
    revision: 'opaque-1',
    text: 'Preserved draft',
    reply_to: null,
  });
  const requests: Command[] = [];
  const storePort = {
    getSnapshot: () => state,
    isSessionRetiring: false,
    sessionEpoch: 1,
    registerSessionCleanup: () => () => {},
    driver: {
      durableMessageDrafts: true,
      request: async (request: Command) => {
        requests.push(request);
        assert.equal(
          request.op,
          'message_draft',
          'presentation must not send, save or change membership',
        );
        return load();
      },
    },
  };
  // Only the store/native boundary is a port; service, controller and access projection are real.
  const drafts = new NormalMessageDraftService(storePort as unknown as ClientStore);
  let language: 'en' | 'pt-BR' = 'pt-BR';
  const jsx = (type: unknown, props: Element['props']) => ({ type, props });
  const modules: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { Text: 'Text', View: 'View' },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ bottom: 34 }) },
    '../core/provider': { useClient: () => ({ ...state, store: storePort }) },
    '../theme': { useTheme: () => ({ presentation: { chat: { service: { text: '#fff' } } } }) },
    '../i18n/index.ts': {
      t: (key: string) => {
        assert.equal(key, 'chat.noLongerAMember');
        return chat[language].noLongerAMember;
      },
    },
  };
  const module = {
    exports: {} as { ConversationComposerAccess(props: Record<string, unknown>): unknown },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(Object.hasOwn(modules, path), `Unexpected native port: ${path}`);
      return modules[path];
    },
  });
  return {
    drafts,
    requests,
    store: storePort,
    update: (change: (current: ClientState) => ClientState) => {
      state = change(state);
    },
    load: (next: typeof load) => {
      load = next;
    },
    language: (next: typeof language) => {
      language = next;
    },
    removeSelf: () => {
      state = {
        ...state,
        snapshot: {
          ...state.snapshot!,
          conversations: state.snapshot!.conversations.map((space) => ({
            ...space,
            members: space.members.filter((member) => member.id !== 'me'),
          })),
        },
      };
    },
    render: (children: unknown, searching = false) =>
      module.exports.ConversationComposerAccess({
        spaceId: 'retained-chat',
        children,
        searching,
      }),
  };
}
function notice(value: unknown): Element {
  const root = value as Element;
  assert.equal(root.type, 'View');
  assert.equal(root.props.testID, 'conversation-read-only');
  return root.props.children as Element;
}

for (const threadRootId of [null, 'thread-root']) {
  test(`${threadRootId ? 'thread' : 'main'} membership loss presents retained history as read-only without erasing the loaded draft`, async () => {
    const f = fixture();
    const child = { type: 'MessageComposer', props: { draft: 'original' } };
    const channel = f.drafts.channel('retained-chat', threadRootId)!;
    await channel.controller.activate(channel.scope);
    assert.equal(f.render(child), child);
    assert.equal(f.requests.length, 1);
    assert.equal(f.requests[0]!.op, 'message_draft');
    assert.equal((f.requests[0] as { thread_root_id: unknown }).thread_root_id, threadRootId);
    f.removeSelf();
    assert.equal(f.drafts.channel('retained-chat', threadRootId), null);
    assert.equal(f.drafts.isCurrent(channel), false);
    assert.equal(notice(f.render(child)).props.children, 'Você não faz mais parte desta conversa.');
    assert.equal(channel.controller.getSnapshot(channel.scope).text, 'Preserved draft');
    assert.equal(f.requests.length, 1, 'read-only presentation performs no native operation');
    f.language('en');
    assert.equal(notice(f.render(child)).props.children, 'You’re no longer a member of this chat.');
  });
}

test('an actual member draft loading, native load failure and retry retain their exact input/error subtree', async () => {
  const f = fixture();
  const pending = deferred<Draft>();
  f.load(() => pending.promise);
  const channel = f.drafts.channel('retained-chat', null)!;
  const work = channel.controller.activate(channel.scope);
  const loading = {
    type: 'MessageComposer',
    props: { draft: channel.controller.getSnapshot(channel.scope) },
  };
  assert.equal(loading.props.draft.loading, true);
  assert.equal(f.render(loading), loading);
  pending.reject(new Error('Synthetic native draft load failure'));
  await assert.rejects(work, /Synthetic native draft load failure/);
  const failed = {
    type: 'MessageComposer',
    props: { draft: channel.controller.getSnapshot(channel.scope) },
  };
  assert.equal(failed.props.draft.error?.kind, 'load');
  assert.equal(failed.props.draft.loaded, false);
  assert.equal(f.render(failed), failed, 'existing error and retry controls survive unchanged');
  f.load(async () => ({ revision: 'retry-ok', text: 'Recovered draft', reply_to: null }));
  await channel.controller.retry(channel.scope);
  assert.equal(channel.controller.getSnapshot(channel.scope).loaded, true);
  assert.equal(channel.controller.getSnapshot(channel.scope).text, 'Recovered draft');
  assert.equal(f.render(failed), failed);
  assert.equal(f.requests.length, 2);
});

test('unknown conversation, session loading/failure, locked owner and identity disagreement never claim lost membership', () => {
  for (const scenario of [
    'unknown',
    'opening',
    'failed',
    'erase',
    'locked',
    'setup',
    'mismatch',
    'retiring',
  ] as const) {
    const f = fixture();
    f.removeSelf();
    f.update((state) => {
      const snapshot = state.snapshot!;
      if (scenario === 'unknown') return { ...state, snapshot: { ...snapshot, conversations: [] } };
      if (scenario === 'opening' || scenario === 'failed') return { ...state, phase: scenario };
      if (scenario === 'erase') return { ...state, erasePending: true };
      if (scenario === 'locked')
        return {
          ...state,
          snapshot: { ...snapshot, account: { ...snapshot.account!, unlocked: false } },
        };
      if (scenario === 'setup')
        return {
          ...state,
          snapshot: { ...snapshot, account: { ...snapshot.account!, setup_pending: true } },
        };
      if (scenario === 'mismatch')
        return {
          ...state,
          snapshot: { ...snapshot, me: { ...snapshot.me!, id: 'another-owner' } },
        };
      f.store.isSessionRetiring = true;
      return state;
    });
    const child = { type: 'ExistingLoadingOrError', props: { scenario } };
    assert.equal(f.render(child), child, scenario);
    assert.equal(f.requests.length, 0);
  }
});

test('query-only search remains available over retained local history without admitting a draft channel', () => {
  const f = fixture();
  f.removeSelf();
  const query = { type: 'MessageComposer', props: { search: true, onSend: null } };
  assert.equal(f.render(query, true), query);
  assert.equal(f.drafts.channel('retained-chat', null), null);
  assert.equal(f.requests.length, 0);
  assert.equal(notice(f.render(query)).props.children, 'Você não faz mais parte desta conversa.');
});
