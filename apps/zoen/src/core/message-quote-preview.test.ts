import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { NormalMessageDrafts, type NormalDraftScope } from './normal-message-drafts.ts';
import type { MessageDraft, MessageDraftAction, MessageDraftAttempt } from './message-draft.ts';
import type { ClientState } from './store';
import type { Snapshot, TimelineEntry } from './types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const sourcePath = fileURLToPath(new URL('./use-message-draft.ts', import.meta.url));
const source = readFileSync(process.env.ZOEN_QUOTE_PREVIEW_SOURCE ?? sourcePath, 'utf8');
const compiled = transformSync(source, {
  filename: sourcePath,
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
  caller: {
    name: 'metro',
    platform: 'ios',
    engine: 'hermes',
    isDev: false,
    supportsReactCompiler: true,
  },
}).code;
const ports = new Set([
  'react',
  'react/compiler-runtime',
  'react-native',
  'expo-router',
  './provider',
]);
const bundled = await build({
  stdin: { contents: compiled, resolveDir: dirname(sourcePath), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'quote-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
const scope: NormalDraftScope = {
  owner: { identityId: 'synthetic-owner', deviceId: 'phone', sessionEpoch: 1, mode: 'account' },
  spaceId: 'synthetic-chat',
  threadRootId: null,
};
const message = (
  text = 'Original synthetic quote',
  patch: Partial<TimelineEntry> = {},
): TimelineEntry => ({
  id: 'quoted-message',
  seq: '1',
  author: { id: 'synthetic-peer', name: 'QA' } as TimelineEntry['author'],
  at_ms: '1',
  kind: { Message: { text, card: null } },
  delivery: 'Sent',
  reply_to: null,
  in_thread: null,
  thread_replies: 0,
  edited: false,
  deleted: false,
  forwarded: false,
  reactions: [],
  voice: null,
  ...patch,
});
const text = (entry: TimelineEntry | null) =>
  entry && 'Message' in entry.kind ? entry.kind.Message.text : null;
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const drain = () => new Promise<void>((done) => setImmediate(done));
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
type Result = {
  messageDraft: MessageDraft;
  replyTo: string | null;
  capture(): MessageDraftAttempt | null;
  updateDraft(action: MessageDraftAction): void;
};

async function fixture(
  initial: readonly TimelineEntry[] = [message()],
  initialRead = async () => message(),
) {
  const controller = new NormalMessageDrafts({
    load: async () => ({
      revision: '1',
      text: 'Preserved normal draft',
      reply_to: 'quoted-message',
    }),
    save: async (_scope, revision, text, reply_to) => ({
      revision: `${revision}x`,
      text,
      reply_to,
    }),
    send: async () => assert.fail('Quote preview must not send content'),
  });
  await controller.activate(scope);
  const channel = { controller, scope };
  let admitted = true;
  let live: ClientState = {
    phase: 'ready',
    erasePending: false,
    mode: 'account',
    snapshot: { account: { identity_id: 'synthetic-owner', device_id: 'phone' } } as Snapshot,
    timelines: { [scope.spaceId]: { entries: [...initial], before: null, has_more: false } },
    notice: null,
    activity: {} as ClientState['activity'],
  };
  let read = initialRead;
  let reads = 0;
  let writes = 0;
  const listeners = new Set<() => void>();
  const slots: Slot[] = [];
  const caches: unknown[][] = [];
  let cursor = 0;
  let cacheCursor = 0;
  let layout: (() => void)[] = [];
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    Boolean(a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i])));
  const memo = (calculate: () => unknown, deps: readonly unknown[]) => {
    const current = slot();
    if (!same(current.deps, deps)) {
      current.deps = deps;
      current.value = calculate();
    }
    return current.value;
  };
  const effect = (callback: Effect, deps?: readonly unknown[], isLayout = false) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    (isLayout ? layout : effects).push(() => {
      current.cleanup?.();
      current.cleanup = callback() ?? undefined;
    });
  };
  const store = {
    getSnapshot: () => live,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    driver: {
      request: async () => {
        ++reads;
        return read();
      },
    },
    refresh: async () => assert.fail('Quote preview must not refresh the store'),
    messageDrafts: {
      channel: () => (admitted ? channel : null),
      isCurrent: (value: unknown) => admitted && value === channel,
    },
  };
  const modules: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => {
        const current = slot();
        current.value ??= { current: value };
        return current.value;
      },
      useState: (value: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = value;
        return [
          current.value,
          (next: unknown) => {
            ++writes;
            current.value = typeof next === 'function' ? next(current.value) : next;
          },
        ];
      },
      useReducer: (reduce: (state: unknown, action: unknown) => unknown, value: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = value;
        return [
          current.value,
          (action: unknown) => {
            ++writes;
            current.value = reduce(current.value, action);
          },
        ];
      },
      useMemo: memo,
      useCallback: (callback: unknown, deps: readonly unknown[]) => memo(() => callback, deps),
      useSyncExternalStore: (_subscribe: unknown, get: () => unknown) => get(),
      useEffect: (callback: Effect, deps?: readonly unknown[]) => effect(callback, deps),
      useLayoutEffect: (callback: Effect, deps?: readonly unknown[]) =>
        effect(callback, deps, true),
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (caches[cacheCursor++] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    'react-native': { AppState: { addEventListener: () => ({ remove() {} }) } },
    'expo-router': { useFocusEffect: (callback: Effect) => effect(callback, [callback]) },
    './provider': { useClient: () => ({ ...live, store }) },
  };
  const module = { exports: {} as { useMessageDraft(space: string, thread?: string): Result } };
  runInNewContext(bundled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected quote native port: ${name}`);
      return modules[name];
    },
  });
  const render = () => {
    cursor = 0;
    cacheCursor = 0;
    layout = [];
    effects = [];
    const result = module.exports.useMessageDraft(scope.spaceId);
    for (const commit of layout) commit();
    for (const commit of effects) commit();
    return result;
  };
  const emit = () => {
    for (const listener of listeners) listener();
  };
  return {
    render,
    controller,
    reads: () => reads,
    writes: () => writes,
    read: (next: typeof read) => {
      read = next;
    },
    cache: (entries: readonly TimelineEntry[]) => {
      live = {
        ...live,
        timelines: { [scope.spaceId]: { entries: [...entries], before: null, has_more: false } },
      };
      emit();
    },
    refresh: () => {
      live = { ...live, snapshot: { ...live.snapshot! } };
      emit();
    },
    admission: (value: boolean) => {
      admitted = value;
      emit();
    },
    unmount: () => {
      for (const current of slots) current.cleanup?.();
    },
    settle: async () => {
      await drain();
      return render();
    },
  };
}

test('compiled quote preview follows cached edits without changing the durable draft or quote target', async () => {
  const f = await fixture();
  assert.equal(text(f.render().messageDraft.reply), 'Original synthetic quote');
  await f.settle();
  const edited = message('Updated synthetic quote', { edited: true });
  f.read(async () => edited);
  f.cache([edited]);
  assert.equal(text(f.render().messageDraft.reply), 'Updated synthetic quote');
  const result = await f.settle();
  assert.equal(result.messageDraft.text, 'Preserved normal draft');
  assert.equal(result.replyTo, 'quoted-message');
});

test('cached tombstones immediately hide stale text and voice metadata while preserving the reply ID', async () => {
  const pending = deferred<TimelineEntry>();
  const f = await fixture([message()], () => pending.promise);
  f.render();
  f.cache([
    message('Old body must not remain visible', {
      deleted: true,
      voice: {} as TimelineEntry['voice'],
    }),
  ]);
  assert.equal(f.render().messageDraft.reply, null);
  pending.resolve(message());
  const result = await f.settle();
  assert.equal(result.messageDraft.reply, null);
  assert.equal(result.replyTo, 'quoted-message');
  assert.equal(result.messageDraft.text, 'Preserved normal draft');
});

test('an uncached quote refreshes on canonical snapshots and a missing read clears only its preview', async () => {
  const f = await fixture([]);
  f.render();
  assert.ok((await f.settle()).messageDraft.reply);
  f.read(async () => {
    throw new Error('Synthetic missing message');
  });
  f.refresh();
  f.render();
  const missing = await f.settle();
  assert.equal(missing.messageDraft.reply, null);
  assert.equal(missing.replyTo, 'quoted-message');
  assert.equal(missing.messageDraft.text, 'Preserved normal draft');
  f.read(async () => message('Recovered canonical preview'));
  f.refresh();
  f.render();
  assert.ok((await f.settle()).messageDraft.reply);
});

test('late reads cannot resurrect a quote after cache deletion, selection ABA or owner/access round trips', async () => {
  for (const interrupt of ['cache', 'reply-aba', 'access'] as const) {
    const pending = deferred<TimelineEntry>();
    const f = await fixture([], () => pending.promise);
    f.render();
    if (interrupt === 'cache') f.cache([message('', { deleted: true })]);
    else if (interrupt === 'reply-aba') {
      f.controller.setReply(scope, null);
      f.controller.setReply(scope, 'quoted-message');
    } else {
      f.admission(false);
      f.admission(true);
    }
    const writes = f.writes();
    pending.resolve(message('Detached plaintext'));
    await drain();
    assert.equal(f.writes(), writes, interrupt);
    f.unmount();
  }
});

test('unmount and changed owners detach pending reads; normal typing does not refetch quotes', async () => {
  for (const interrupt of ['unmount', 'owner'] as const) {
    const pending = deferred<TimelineEntry>();
    const f = await fixture([], () => pending.promise);
    f.render();
    if (interrupt === 'unmount') f.unmount();
    else f.admission(false);
    const writes = f.writes();
    pending.resolve(message());
    await drain();
    assert.equal(f.writes(), writes);
  }
  const f = await fixture();
  f.render();
  await f.settle();
  const reads = f.reads();
  f.controller.setText(scope, 'New normal typing');
  assert.equal(f.render().messageDraft.text, 'New normal typing');
  assert.equal(f.reads(), reads);
});

test('a missing canonical quote does not resurface from the same stale cache during a delayed refresh', async () => {
  const f = await fixture([message()], async () => {
    throw new Error('Synthetic missing original');
  });
  f.render();
  assert.equal((await f.settle()).messageDraft.reply, null);
  const pending = deferred<TimelineEntry>();
  f.read(() => pending.promise);
  f.refresh();
  assert.equal(f.render().messageDraft.reply, null);
  pending.resolve(message('', { deleted: true }));
  assert.equal((await f.settle()).messageDraft.reply, null);
});

test('capture reads the latest normal text and quote after native drain updates before a React commit', async () => {
  const f = await fixture();
  const actions = f.render();
  const renderedText = actions.messageDraft.text;
  actions.updateDraft({ type: 'reply', entry: message('New quote', { id: 'new-quote' }) });
  actions.updateDraft({ type: 'text', value: 'Latest native normal text 🌤️' });
  const captured = actions.capture();
  assert.ok(captured);
  assert.equal(
    actions.messageDraft.text,
    renderedText,
    'the rendered projection has not committed yet',
  );
  assert.equal(captured.text, 'Latest native normal text 🌤️');
  assert.equal(captured.replyId, 'new-quote');
  assert.equal(captured.editingId, null);
  assert.equal(captured.contextRevision, 1);
  assert.equal(captured.textRevision, f.controller.getSnapshot(scope).textRevision);
});

test('capture reads the latest edit buffer and revisions without waiting for React to render', async () => {
  const f = await fixture();
  const original = f.render();
  original.updateDraft({ type: 'edit', entry: message('Editable'), text: 'Editable' });
  const editing = f.render();
  editing.updateDraft({ type: 'text', value: 'Latest native edit text' });
  const captured = editing.capture();
  assert.ok(captured);
  assert.equal(editing.messageDraft.editText, 'Editable');
  assert.equal(captured.text, 'Latest native edit text');
  assert.equal(captured.editingId, 'quoted-message');
  assert.equal(captured.contextRevision, editing.messageDraft.contextRevision);
  assert.equal(captured.editRevision, editing.messageDraft.editRevision + 1);
  assert.equal(captured.textRevision, f.controller.getSnapshot(scope).textRevision);
  assert.equal(f.controller.getSnapshot(scope).text, 'Preserved normal draft');
});

test('capture rejects callbacks retained by a channel that is no longer admitted', async () => {
  const f = await fixture();
  const old = f.render();
  f.admission(false);
  assert.equal(old.capture(), null, 'admission is checked even before the next React render');
  f.render();
  assert.equal(old.capture(), null);
});
