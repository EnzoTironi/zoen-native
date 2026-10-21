import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import type { ClientStore } from './store';
import type { TimelineEntry } from './types';
import type { ConversationMediaRequest } from '../media/types';
import type { previewConversationMedia as PreviewConversationMedia } from '../media/conversation-media-preview';

const output = await build({
  entryPoints: [fileURLToPath(new URL('../media/conversation-media-preview.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'localized-native-port',
      setup(builder) {
        builder.onResolve({ filter: /i18n\/index\.ts$/ }, () => ({
          path: 'i18n',
          namespace: 'gallery-test',
        }));
        builder.onLoad({ filter: /.*/, namespace: 'gallery-test' }, () => ({
          contents: 'export const t = key => key;',
          loader: 'js',
        }));
      },
    },
  ],
});
const { previewConversationMedia } = (await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
)) as { previewConversationMedia: typeof PreviewConversationMedia };
const deferred = () => {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
function entry(id: string): TimelineEntry {
  return {
    id,
    author: { id: 'author', name: 'QA' },
    deleted: false,
    forwarded: false,
    voice: null,
    kind: { Message: { text: id, card: { item_id: `file-${id}`, kind_id: 'file', version: 2 } } },
    media: {
      presentation: 'media',
      group: { id: `album_${'a'.repeat(64)}`, index: id === 'a' ? 0 : 1, count: 2 },
    },
    in_thread: 'thread',
    reply_to: null,
  } as TimelineEntry;
}
function fixture() {
  const close = deferred(),
    shown = deferred();
  let epoch = 1,
    retiring = false;
  const rows = new Map(['a', 'b'].map((id) => [id, entry(id)]));
  const canonical = new Set<(spaces?: readonly string[]) => void>();
  const state = new Set<() => void>();
  const cleanup = new Set<() => Promise<void>>();
  const requests: unknown[] = [],
    previews: ConversationMediaRequest[] = [];
  let signal: AbortSignal | undefined;
  const store = {
    get sessionEpoch() {
      return epoch;
    },
    get isSessionRetiring() {
      return retiring;
    },
    getSnapshot: () => ({ phase: 'ready', erasePending: false }),
    subscribeCanonicalChanges: (f: (spaces?: readonly string[]) => void) => {
      canonical.add(f);
      return () => canonical.delete(f);
    },
    subscribe: (f: () => void) => {
      state.add(f);
      return () => state.delete(f);
    },
    registerSessionCleanup: (f: () => Promise<void>) => {
      cleanup.add(f);
      return () => cleanup.delete(f);
    },
    driver: {
      request: async (request: { message_id: string }) => {
        requests.push(request);
        return structuredClone(rows.get(request.message_id));
      },
      media: {
        previewConversationMedia: async (request: ConversationMediaRequest, owner: AbortSignal) => {
          previews.push(request);
          signal = owner;
          shown.resolve();
          owner.addEventListener('abort', close.resolve, { once: true });
          await close.promise;
        },
      },
    },
  } as unknown as ClientStore;
  return {
    store,
    rows,
    requests,
    previews,
    shown,
    close,
    canonical,
    state,
    cleanup,
    signal: () => signal,
    retire: () => {
      retiring = true;
      ++epoch;
      for (const f of state) f();
    },
  };
}
test('conversation viewer opens chosen child with exact bounded references and no materialization in JS', async () => {
  const f = fixture();
  const work = previewConversationMedia(f.store, 'space', 'b', [
    f.rows.get('b')!,
    f.rows.get('a')!,
  ]);
  await f.shown.promise;
  assert.deepEqual(f.previews, [
    {
      spaceId: 'space',
      selectedId: 'b',
      entries: [
        { messageId: 'b', itemId: 'file-b', version: 2 },
        { messageId: 'a', itemId: 'file-a', version: 2 },
      ],
    },
  ]);
  assert.equal(f.requests.length, 2);
  f.close.resolve();
  await work;
  assert.equal(f.canonical.size + f.state.size + f.cleanup.size, 0);
});
test('unrelated canonical change keeps viewer, changed/deleted canonical child cancels it', async () => {
  const f = fixture();
  const work = previewConversationMedia(f.store, 'space', 'a', [...f.rows.values()]);
  await f.shown.promise;
  for (const change of f.canonical) change(['other']);
  assert.equal(f.requests.length, 2);
  for (const change of f.canonical) change(['space']);
  await new Promise((done) => setImmediate(done));
  assert.equal(f.signal()?.aborted, false);
  f.rows.get('b')!.deleted = true;
  for (const change of f.canonical) change(['space']);
  await work;
  assert.equal(f.signal()?.aborted, true);
});
test('account retirement aborts native viewer and session cleanup waits for native dismissal', async () => {
  const f = fixture();
  const work = previewConversationMedia(f.store, 'space', 'a', [...f.rows.values()]);
  await f.shown.promise;
  const cleanups = [...f.cleanup].map((cleanup) => cleanup());
  assert.equal(f.signal()?.aborted, true);
  await Promise.all([work, ...cleanups]);
  const next = fixture();
  const second = previewConversationMedia(next.store, 'space', 'a', [...next.rows.values()]);
  await next.shown.promise;
  next.retire();
  await second;
  assert.equal(next.signal()?.aborted, true);
});
test('stale borrowed projection is rejected before native admission and >10 candidates never query', async () => {
  const f = fixture();
  const borrowed = structuredClone(f.rows.get('a')!);
  if ('Message' in borrowed.kind) borrowed.kind.Message.card!.version = 1;
  await previewConversationMedia(f.store, 'space', 'a', [borrowed]);
  assert.equal(f.previews.length, 0);
  await assert.rejects(
    previewConversationMedia(
      f.store,
      'space',
      'a',
      Array.from({ length: 11 }, () => borrowed),
    ),
  );
  assert.equal(f.requests.length, 1);
});
