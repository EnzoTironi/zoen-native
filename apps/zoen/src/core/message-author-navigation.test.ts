import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { ClientState } from './store';
import type { Snapshot, TimelineEntry } from './types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const sourcePath = fileURLToPath(new URL('../components/use-message-actions.tsx', import.meta.url));
const source = readFileSync(process.env.ZOEN_MESSAGE_AUTHOR_SOURCE ?? sourcePath, 'utf8');
// Exercise the shipped iOS compiler path as well as the actual hook. Native
// ports are replaced below; canonical interaction and membership policies are not.
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
const compilerUsesCache = compiled.includes('react/compiler-runtime');
const externalPorts = new Set([
  'react',
  'react/compiler-runtime',
  'react/jsx-runtime',
  'expo-router',
  'expo-clipboard',
  '../core/provider',
  '../media/share-message',
  '../i18n/index.ts',
  './dialogs',
  './message-actions',
  './reaction-details',
]);
const bundled = await build({
  stdin: { contents: compiled, resolveDir: dirname(sourcePath), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'message-author-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          externalPorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

const spaceId = 'synthetic_group';
function author(id = 'sender', kind: 'Person' | 'Agent' = 'Person'): TimelineEntry['author'] {
  return {
    id,
    kind,
    name: id,
    handle: id,
    initials: 'QA',
    tint_hex: '#487a64',
    glyph: null,
    bio: '',
    owner_id: null,
    owner_name: null,
    owner_tint_hex: null,
    photo_sha256: null,
    photo_ready: false,
    photo_mime: null,
    is_me: false,
    is_mine: false,
  };
}
function message(patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: 'synthetic_message',
    seq: '1',
    author: author(),
    at_ms: '1000',
    kind: { Message: { text: 'Synthetic author-navigation fixture', card: null } },
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
  };
}
function clientState(): ClientState {
  return {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      me: author('alice'),
      account: {
        identity_id: 'alice',
        device_id: 'phone',
        unlocked: true,
        setup_pending: false,
      },
      conversations: [{ id: spaceId, members: [author('alice'), author()] }],
    } as unknown as Snapshot,
    timelines: {},
    notice: null,
    activity: {} as ClientState['activity'],
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
type Options = { spaceId: string; entries: readonly TimelineEntry[]; disabled: boolean };
type Actions = {
  profile(entry: TimelineEntry): Promise<void>;
  react(entry: TimelineEntry, emoji: string | null): Promise<void>;
};
type Route = { pathname: string; params: { id: string } };
type Read = { op: string; space_id: string; message_id: string };

function fixture() {
  let live = clientState();
  let rendered = live;
  let options: Options = { spaceId, entries: [message()], disabled: false };
  let read: (command: Read) => Promise<TimelineEntry> = async () => message();
  let mutate: () => Promise<unknown> = async () => undefined;
  let push: (route: Route) => void = () => {};
  const reads: Read[] = [];
  const mutations: unknown[] = [];
  const routes: Route[] = [];
  const alerts: unknown[][] = [];
  const writes: unknown[] = [];
  const listeners = new Set<() => void>();
  const slots: Slot[] = [];
  const compilerCaches: unknown[][] = [];
  let cursor = 0;
  let compilerCursor = 0;
  let compiledHookCalls = 0;
  let layoutEffects: (() => void)[] = [];
  let effects: (() => void)[] = [];
  let focused = true;
  let focusEffect: Effect | undefined;
  let nextFocusEffect: Effect | undefined;
  let focusCleanup: (() => void) | undefined;
  const depsEqual = (left: readonly unknown[] | undefined, right: readonly unknown[] | undefined) =>
    Boolean(
      left &&
      right &&
      left.length === right.length &&
      left.every((value, i) => Object.is(value, right[i])),
    );
  const slot = () => (slots[cursor++] ??= {});
  const effect = (callback: Effect, deps: readonly unknown[] | undefined, layout = false) => {
    const current = slot();
    if (depsEqual(current.deps, deps)) return;
    current.deps = deps;
    (layout ? layoutEffects : effects).push(() => {
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
      request: (command: Read) => {
        reads.push({ ...command });
        return read(command);
      },
    },
    request: (command: unknown) => {
      mutations.push(command);
      return mutate();
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
            current.value = typeof next === 'function' ? next(current.value) : next;
            writes.push(current.value);
          },
        ];
      },
      useCallback: (callback: unknown, deps: readonly unknown[]) => {
        const current = slot();
        if (!depsEqual(current.deps, deps)) {
          current.deps = deps;
          current.value = callback;
        }
        return current.value;
      },
      useEffect: (callback: Effect, deps: readonly unknown[] | undefined) => effect(callback, deps),
      useLayoutEffect: (callback: Effect, deps: readonly unknown[] | undefined) =>
        effect(callback, deps, true),
    },
    'react/compiler-runtime': {
      c: (size: number) => {
        ++compiledHookCalls;
        return (compilerCaches[compilerCursor++] ??= Array(size).fill(
          Symbol.for('react.memo_cache_sentinel'),
        ));
      },
    },
    'react/jsx-runtime': { jsx: () => null, jsxs: () => null },
    'expo-router': {
      router: {
        push: (route: Route) => {
          push(route);
          routes.push({ pathname: route.pathname, params: { id: route.params.id } });
        },
      },
      useFocusEffect: (callback: Effect) => {
        nextFocusEffect = callback;
      },
    },
    'expo-clipboard': {
      setStringAsync: () => assert.fail('Profile navigation must not copy content'),
    },
    '../core/provider': { useClient: () => ({ ...rendered, store }) },
    '../media/share-message': {
      shareMessageMedia: () => assert.fail('Profile navigation must not export media'),
    },
    '../i18n/index.ts': { t: (key: string) => key, locale: 'en' },
    './dialogs': {
      useDialogs: () => ({
        confirm: () => assert.fail('Profile navigation must not ask for mutation confirmation'),
        inform: (...args: unknown[]) => {
          alerts.push(args);
          return Promise.resolve();
        },
      }),
    },
    './message-actions': { MessageActions: () => null },
    './reaction-details': { ReactionDetails: () => null },
  };
  const module = {
    exports: {} as {
      useMessageActions(options: Options & { onReply(): void; onEdit(): void }): Actions;
    },
  };
  runInNewContext(bundled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    // Driver and router errors belong to the same JS realm as the hook in-app.
    Error,
    require: (name: string) => {
      assert.ok(
        Object.hasOwn(modules, name),
        `Unexpected native import in author-navigation test: ${name}`,
      );
      return modules[name];
    },
  });
  const render = (patch: Partial<Options> = {}) => {
    options = { ...options, ...patch };
    rendered = live;
    cursor = 0;
    compilerCursor = 0;
    layoutEffects = [];
    effects = [];
    const result = module.exports.useMessageActions({ ...options, onReply() {}, onEdit() {} });
    // Some hooks legitimately bail out of compiler memoization. If a later
    // transform emits caches, keep them persistent rather than masking them.
    if (compilerUsesCache) assert.ok(compiledHookCalls > 0);
    for (const commit of layoutEffects) commit();
    for (const commit of effects) commit();
    if (focusEffect !== nextFocusEffect) {
      focusCleanup?.();
      focusEffect = nextFocusEffect;
      focusCleanup = focused ? (focusEffect?.() ?? undefined) : undefined;
    }
    return result;
  };
  return {
    actions: render(),
    reads,
    mutations,
    routes,
    alerts,
    writes,
    render,
    state: () => live,
    update: (change: (state: ClientState) => ClientState) => {
      live = change(live);
      for (const listener of listeners) listener();
    },
    read: (next: typeof read) => {
      read = next;
    },
    mutate: (next: typeof mutate) => {
      mutate = next;
    },
    push: (next: typeof push) => {
      push = next;
    },
    blur: () => {
      focused = false;
      focusCleanup?.();
      focusCleanup = undefined;
    },
    focus: () => {
      focused = true;
      focusCleanup = focusEffect?.() ?? undefined;
    },
    unmount: () => {
      for (const current of slots) current.cleanup?.();
      focusCleanup?.();
      focusCleanup = undefined;
    },
  };
}

type Change = (state: ClientState) => ClientState;
const account =
  (patch: Partial<NonNullable<Snapshot['account']>>): Change =>
  (state) => ({
    ...state,
    snapshot: { ...state.snapshot!, account: { ...state.snapshot!.account!, ...patch } },
  });
const invalidSessions: [string, Change][] = [
  ['other owner', account({ identity_id: 'bob' })],
  ['other device', account({ device_id: 'other_phone' })],
  ['other mode', (state) => ({ ...state, mode: 'demo' })],
  ['opening', (state) => ({ ...state, phase: 'opening' })],
  ['erasing', (state) => ({ ...state, phase: 'erasing' })],
  ['onboarding', (state) => ({ ...state, phase: 'onboarding' })],
  ['failed', (state) => ({ ...state, phase: 'failed' })],
  ['erase pending', (state) => ({ ...state, erasePending: true })],
  ['locked account', account({ unlocked: false })],
  ['unfinished account', account({ setup_pending: true })],
  ['missing snapshot', (state) => ({ ...state, snapshot: null })],
  [
    'mismatched current identity',
    (state) => ({ ...state, snapshot: { ...state.snapshot!, me: author('bob') } }),
  ],
  [
    'removed source chat',
    (state) => ({ ...state, snapshot: { ...state.snapshot!, conversations: [] } }),
  ],
  [
    'removed membership',
    (state) => ({
      ...state,
      snapshot: {
        ...state.snapshot!,
        conversations: state.snapshot!.conversations.map((chat) => ({
          ...chat,
          members: [author()],
        })),
      },
    }),
  ],
];

test('author profile rereads canonical human and agent identities without changing messages', async () => {
  for (const kind of ['Person', 'Agent'] as const) {
    const f = fixture();
    f.read(async () => message({ author: author(`canonical_${kind}`, kind) }));
    await f.actions.profile(message({ author: author('stale_recycled_author') }));
    assert.deepEqual(f.reads, [
      { op: 'message', space_id: spaceId, message_id: 'synthetic_message' },
    ]);
    assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: `canonical_${kind}` } }]);
    assert.deepEqual(f.mutations, []);
    assert.deepEqual(f.alerts, []);
  }
});

test('a duplicate author press shares one read and accepted navigation stays latched until focus changes', async () => {
  const f = fixture();
  const read = deferred<TimelineEntry>();
  f.read(() => read.promise);
  const first = f.actions.profile(message());
  await f.actions.profile(message());
  assert.equal(f.reads.length, 1);
  read.resolve(message());
  await first;
  await f.render().profile(message());
  assert.equal(f.reads.length, 1);
  assert.equal(f.routes.length, 1);
  f.blur();
  f.focus();
  await f.render().profile(message());
  assert.equal(f.reads.length, 2);
  assert.equal(f.routes.length, 2);
});

test('saved author callbacks recheck live session and membership before any canonical read', async (t) => {
  for (const [name, change] of invalidSessions) {
    await t.test(name, async () => {
      const f = fixture();
      f.update(change);
      await f.actions.profile(message());
      assert.deepEqual(f.reads, []);
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.alerts, []);
    });
  }
});

test('disabled, missing, deleted and non-message canonical rows reject saved author callbacks', async () => {
  for (const patch of [
    { disabled: true },
    { entries: [] },
    { entries: [message({ deleted: true })] },
    { entries: [message({ kind: { System: { text: 'Synthetic system event' } } })] },
  ]) {
    const f = fixture();
    f.render(patch);
    await f.actions.profile(message());
    assert.deepEqual(f.reads, []);
    assert.deepEqual(f.routes, []);
  }
});

test('callbacks captured for another rendered owner, device, mode or source chat remain detached', async () => {
  for (const change of [
    account({ identity_id: 'bob' }),
    account({ device_id: 'new_phone' }),
    invalidSessions[2]![1],
  ]) {
    const f = fixture();
    f.update(change);
    f.render();
    await f.actions.profile(message());
    assert.deepEqual(f.reads, []);
  }
  const f = fixture();
  f.update((state) => ({
    ...state,
    snapshot: {
      ...state.snapshot!,
      conversations: [
        ...state.snapshot!.conversations,
        { ...state.snapshot!.conversations[0]!, id: 'other_chat' },
      ],
    },
  }));
  f.render({ spaceId: 'other_chat' });
  await f.actions.profile(message());
  assert.deepEqual(f.reads, []);
});

test('blur and unmount reject saved callbacks and suppress pending-read UI work', async () => {
  for (const lifecycle of ['blur', 'unmount'] as const) {
    const f = fixture();
    const read = deferred<TimelineEntry>();
    f.read(() => read.promise);
    const pending = f.actions.profile(message());
    f[lifecycle]();
    const writes = f.writes.length;
    await f.actions.profile(message());
    assert.equal(f.reads.length, 1);
    read.resolve(message());
    await pending;
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.alerts, []);
    assert.equal(f.writes.length, writes);
  }
});

test('canonical read completion rechecks fresh session and source access without needing a render', async (t) => {
  for (const [name, change] of invalidSessions) {
    await t.test(name, async () => {
      const f = fixture();
      const read = deferred<TimelineEntry>();
      f.read(() => read.promise);
      const pending = f.actions.profile(message());
      f.update(change);
      const writes = f.writes.length;
      read.resolve(message());
      await pending;
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.alerts, []);
      assert.equal(f.writes.length, writes);
    });
  }
});

test('transient session or membership loss cannot revive a pending read after access returns', async (t) => {
  for (const [name, change] of invalidSessions) {
    await t.test(name, async () => {
      const f = fixture();
      const original = f.state();
      const oldRead = deferred<TimelineEntry>();
      const newRead = deferred<TimelineEntry>();
      f.read(() => oldRead.promise);
      const oldAttempt = f.actions.profile(message());
      f.update(change);
      f.update(() => original);
      f.read(() => newRead.promise);
      const newAttempt = f.actions.profile(message());
      assert.equal(f.reads.length, 2, 'A restored session can start a fresh attempt');
      oldRead.resolve(message({ author: author('detached_author') }));
      await oldAttempt;
      await f.actions.profile(message());
      assert.equal(f.reads.length, 2, 'Detached finally cannot release the new attempt');
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.alerts, []);
      newRead.resolve(message({ author: author('restored_author') }));
      await newAttempt;
      assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: 'restored_author' } }]);
    });
  }
});

test('source-chat and disabled-state round trips detach old reads before a new attempt', async () => {
  for (const transition of ['source', 'disabled'] as const) {
    const f = fixture();
    f.update((state) => ({
      ...state,
      snapshot: {
        ...state.snapshot!,
        conversations: [
          ...state.snapshot!.conversations,
          { ...state.snapshot!.conversations[0]!, id: 'other_chat' },
        ],
      },
    }));
    const oldRead = deferred<TimelineEntry>();
    const newRead = deferred<TimelineEntry>();
    f.read(() => oldRead.promise);
    const oldAttempt = f.actions.profile(message());
    if (transition === 'source') f.render({ spaceId: 'other_chat' });
    else f.render({ disabled: true });
    const actions = f.render({ spaceId, disabled: false });
    f.read(() => newRead.promise);
    const newAttempt = actions.profile(message());
    oldRead.reject(new Error('Synthetic detached round-trip read failure'));
    await oldAttempt;
    await actions.profile(message());
    assert.equal(f.reads.length, 2);
    assert.deepEqual(f.alerts, []);
    assert.deepEqual(f.routes, []);
    newRead.resolve(message());
    await newAttempt;
    assert.equal(f.routes.length, 1);
  }
});

test('canonical read cannot reopen a row disabled, deleted or removed while the read was pending', async () => {
  for (const patch of [
    { disabled: true },
    { entries: [] },
    { entries: [message({ deleted: true })] },
  ]) {
    const f = fixture();
    const read = deferred<TimelineEntry>();
    f.read(() => read.promise);
    const pending = f.actions.profile(message());
    f.render(patch);
    read.resolve(message());
    await pending;
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.alerts, []);
  }
});

test('malformed or noninteractive canonical results report the failure and permit a valid retry', async () => {
  const invalid = [
    message({ id: 'different_message' }),
    message({ deleted: true }),
    message({ kind: { System: { text: 'Synthetic system event' } } }),
    message({ author: author('   ') }),
    { ...message(), author: null } as unknown as TimelineEntry,
  ];
  for (const canonical of invalid) {
    const f = fixture();
    f.read(async () => canonical);
    await f.actions.profile(message());
    assert.deepEqual(f.routes, []);
    assert.equal(f.alerts.length, 1);
    assert.equal(f.alerts[0]![0], 'chat.profileUnavailable');
    f.read(async () => message({ author: author('valid_retry') }));
    await f.actions.profile(message());
    assert.equal(f.reads.length, 2);
    assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: 'valid_retry' } }]);
  }
});

test('read errors alert only the active attempt and a same-focus retry is available', async () => {
  const f = fixture();
  f.read(async () => {
    throw new Error('Synthetic profile read failure');
  });
  await f.actions.profile(message());
  assert.equal(f.alerts.length, 1);
  assert.equal(f.alerts[0]![1], 'Synthetic profile read failure');
  f.read(async () => message());
  await f.actions.profile(message());
  assert.equal(f.routes.length, 1);
  assert.equal(f.reads.length, 2);
  for (const lifecycle of ['blur', 'unmount'] as const) {
    const stale = fixture();
    const read = deferred<TimelineEntry>();
    stale.read(() => read.promise);
    const pending = stale.actions.profile(message());
    stale[lifecycle]();
    read.reject(new Error('Synthetic detached read failure'));
    await pending;
    assert.deepEqual(stale.alerts, []);
    assert.deepEqual(stale.routes, []);
  }
});

test('an old success or failure after blur/refocus cannot navigate or clear the newer attempt latch', async () => {
  for (const outcome of ['resolve', 'reject'] as const) {
    const f = fixture();
    const oldRead = deferred<TimelineEntry>();
    const newRead = deferred<TimelineEntry>();
    f.read(() => oldRead.promise);
    const oldAttempt = f.actions.profile(message());
    f.blur();
    f.focus();
    f.read(() => newRead.promise);
    const actions = f.render();
    const newAttempt = actions.profile(message());
    if (outcome === 'resolve') oldRead.resolve(message({ author: author('stale_author') }));
    else oldRead.reject(new Error('Synthetic stale read failure'));
    await oldAttempt;
    await actions.profile(message());
    assert.equal(f.reads.length, 2, 'Old finally must not release the active attempt');
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.alerts, []);
    newRead.resolve(message({ author: author('fresh_author') }));
    await newAttempt;
    assert.deepEqual(f.routes, [{ pathname: '/person/[id]', params: { id: 'fresh_author' } }]);
  }
});

test('a synchronous router failure releases the attempt for retry without a second canonical mutation', async () => {
  const f = fixture();
  f.push(() => {
    throw new Error('Synthetic unavailable navigator');
  });
  await f.actions.profile(message());
  assert.deepEqual(f.routes, []);
  assert.equal(f.alerts.length, 1);
  assert.equal(f.alerts[0]![1], 'Synthetic unavailable navigator');
  f.push(() => {});
  await f.actions.profile(message());
  assert.equal(f.routes.length, 1);
  assert.equal(f.reads.length, 2);
  assert.deepEqual(f.mutations, []);
});

test('an in-flight message mutation defers author navigation until the mutation completes', async () => {
  const f = fixture();
  const mutation = deferred<unknown>();
  f.mutate(() => mutation.promise);
  const reaction = f.actions.react(message(), '👍');
  await f.actions.profile(message());
  assert.deepEqual(f.reads, []);
  mutation.resolve(undefined);
  await reaction;
  await f.actions.profile(message());
  assert.equal(f.routes.length, 1);
  assert.equal(f.mutations.length, 1);
});
