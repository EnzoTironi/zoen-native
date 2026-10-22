// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  attachment,
  messagingCore,
  conversation,
  linked,
  vault,
  authenticate,
  mediaMessages,
  settle,
  login,
  socket,
} from './fixture.ts';
import { type FileDoc, Messages, type MessagesState } from '../messages.ts';
import { type CoreOperation } from '../worker-client.ts';
import assert from 'node:assert/strict';

export const scenario11 = async (t: TestContext) => {
  install(t);
  const file = await attachment();
  const head = new TextEncoder().encode(
    JSON.stringify({ doc: file, uploads: [{ blob: file.chunks[0]!.blob, len: 1 }] }),
  );
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true);
  framed.set(head, 4);
  const voice = { ms: 2300, levels: '04ff80', transcript: '', reply: null };
  const codec = messagingCore();
  let queued: FileDoc | undefined;
  const core = {
    ...codec,
    bytes: async (op: CoreOperation) => (op.kind === 'seal_file' ? framed : codec.bytes(op)),
    text: async (op: CoreOperation) => {
      if (op.kind !== 'queue_file_record') return codec.text(op);
      queued = JSON.parse(op.doc);
      return JSON.stringify({
        result: { itemId: 'it_fixture', messageId: 'voice' },
        update: { snapshot: 'sent voice', frames: [], conversations: [conversation] },
      });
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: () => {},
  });
  t.after(() => messages.stop());
  t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 200 }));
  await messages.start();
  await authenticate();
  await messages.sendFile('sp_fixture', {
    name: file.name,
    mime: file.mime,
    bytes: new Uint8Array([1, 2, 3]),
    voice,
  });
  assert.deepEqual(queued, { ...file, voice });
};

export const scenario18 = async (t: TestContext) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const file = await attachment();
  const head = new TextEncoder().encode(
    JSON.stringify({ doc: file, uploads: [{ blob: file.chunks[0]!.blob, len: 1 }] }),
  );
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true);
  framed.set(head, 4);
  const messages = mediaMessages(t, async (op) =>
    op.kind === 'seal_file' ? framed : new Uint8Array(),
  );
  for (const status of [429, 500, 503, 0]) {
    const fetching = t.mock.method(globalThis, 'fetch', async () => {
      if (!status) throw new Error('offline');
      return new Response(null, { status });
    });
    const sending = messages.sendFile('sp_fixture', {
      name: file.name,
      mime: file.mime,
      bytes: new Uint8Array([1]),
    });
    const rejected = assert.rejects(sending, /Sem conexão/);
    await settle();
    for (const delay of [800, 1600, 3200]) {
      t.mock.timers.tick(delay);
      await settle();
    }
    await rejected;
    assert.equal(fetching.mock.callCount(), 4);
    fetching.mock.restore();
  }
  for (const status of [403, 413]) {
    const fetching = t.mock.method(globalThis, 'fetch', async () => new Response(null, { status }));
    await assert.rejects(
      messages.sendFile('sp_fixture', {
        name: file.name,
        mime: file.mime,
        bytes: new Uint8Array([1]),
      }),
      status === 413 ? /grande demais/ : /recusou/,
    );
    assert.equal(fetching.mock.callCount(), 1);
    fetching.mock.restore();
  }
  let responses = 0;
  const fetching = t.mock.method(
    globalThis,
    'fetch',
    async () => new Response(null, { status: ++responses < 2 ? 503 : 200 }),
  );
  let done = 0;
  // Upload succeeds after the retry; this unstarted controller then refuses the post.
  const sending = assert.rejects(
    messages.sendFile(
      'sp_fixture',
      { name: file.name, mime: file.mime, bytes: new Uint8Array([1]) },
      () => {
        done++;
      },
    ),
    /Sem conexão agora/,
  );
  await settle();
  t.mock.timers.tick(800);
  await settle();
  await sending;
  assert.equal(fetching.mock.callCount(), 2);
  assert.equal(done, 1);
};

export const scenario19 = async (t: TestContext) => {
  install(t);
  const file = await attachment();
  const head = new TextEncoder().encode(
    JSON.stringify({
      doc: { ...file, voice: { ms: 1200, levels: '04', transcript: '', reply: null } },
      uploads: [{ blob: file.chunks[0]!.blob, len: 1 }],
    }),
  );
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true);
  framed.set(head, 4);
  const voice = { ms: 1200, levels: '04', transcript: '', reply: null };
  const codec = messagingCore();
  const failed: string[] = [];
  const core = {
    ...codec,
    bytes: async (op: CoreOperation) => (op.kind === 'seal_file' ? framed : codec.bytes(op)),
    text: async (op: CoreOperation) => {
      if (op.kind === 'queue_file' || op.kind === 'fail_outbox')
        assert.fail('incomplete media must never be queued for publication');
      if (op.kind === 'queue_failed_file') {
        assert.deepEqual(JSON.parse(op.chunks), { [file.chunks[0]!.blob]: [0] });
        failed.push('pending-voice');
        return JSON.stringify({
          snapshot: 'failed',
          frames: [],
          conversations: [
            {
              ...conversation,
              messages: [
                {
                  id: 'pending-voice',
                  entry: null,
                  author: login.identity,
                  name: 'Me',
                  text: '',
                  at_ms: 1,
                  delivery: 'failed',
                  failure: op.reason,
                  ticks: null,
                  edited: false,
                  deleted: false,
                  forwarded: false,
                  reactions: [],
                  quote: null,
                  thread: null,
                  replies: 0,
                  repliers: [],
                  last_reply_ms: null,
                  file: { ...file, voice },
                  file_item_id: 'it_fixture',
                  file_version: 1,
                },
              ],
            },
          ],
        });
      }
      return codec.text(op);
    },
  };
  const states: MessagesState[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 503 }));
  await messages.start();
  await authenticate();
  const rejected = assert.rejects(
    messages.sendFile('sp_fixture', {
      name: file.name,
      mime: file.mime,
      bytes: new Uint8Array([1]),
      voice,
    }),
  );
  await settle();
  for (const delay of [800, 1600, 3200]) {
    t.mock.timers.tick(delay);
    await settle();
  }
  await rejected;
  assert.deepEqual(failed, ['pending-voice']);
  const state = states.at(-1);
  if (state?.kind !== 'active') assert.fail('expected active');
  const msg = state.conversations[0]!.messages[0]!;
  assert.equal(msg.delivery, 'failed');
  assert.deepEqual(msg.file?.voice, voice);
};

export const scenario20 = async (t: TestContext) => {
  install(t);
  const file = await attachment();
  const head = new TextEncoder().encode(
    JSON.stringify({
      doc: { ...file, voice: { ms: 1200, levels: '04', transcript: '', reply: null } },
      uploads: [{ blob: file.chunks[0]!.blob, len: 1 }],
    }),
  );
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true);
  framed.set(head, 4);
  const voice = { ms: 1200, levels: '04', transcript: '', reply: null };
  const codec = messagingCore();
  const failed: string[] = [];
  const core = {
    ...codec,
    bytes: async (op: CoreOperation) => (op.kind === 'seal_file' ? framed : codec.bytes(op)),
    text: async (op: CoreOperation) => {
      if (op.kind === 'queue_file' || op.kind === 'fail_outbox')
        assert.fail('incomplete media must never be queued for publication');
      if (op.kind === 'queue_failed_file') {
        assert.deepEqual(JSON.parse(op.chunks), { [file.chunks[0]!.blob]: [0] });
        failed.push('pending-voice');
        return JSON.stringify({
          snapshot: 'failed',
          frames: [],
          conversations: [
            {
              ...conversation,
              messages: [
                {
                  id: 'pending-voice',
                  entry: null,
                  author: login.identity,
                  name: 'Me',
                  text: '',
                  at_ms: 1,
                  delivery: 'failed',
                  failure: op.reason,
                  ticks: null,
                  edited: false,
                  deleted: false,
                  forwarded: false,
                  reactions: [],
                  quote: null,
                  thread: null,
                  replies: 0,
                  repliers: [],
                  last_reply_ms: null,
                  file: { ...file, voice },
                  file_item_id: 'it_fixture',
                  file_version: 1,
                },
              ],
            },
          ],
        });
      }
      return codec.text(op);
    },
  };
  const states: MessagesState[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  t.mock.method(globalThis, 'fetch', async () => {
    socket().close();
    return new Response(null, { status: 200 });
  });
  await messages.start();
  await authenticate();
  const rejected = assert.rejects(
    messages.sendFile('sp_fixture', {
      name: file.name,
      mime: file.mime,
      bytes: new Uint8Array([1]),
      voice,
    }),
  );
  await settle();
  for (const delay of [800, 1600, 3200]) {
    t.mock.timers.tick(delay);
    await settle();
  }
  await rejected;
  assert.deepEqual(failed, ['pending-voice']);
  const state = states.at(-1);
  if (state?.kind !== 'active') assert.fail('expected active');
  const msg = state.conversations[0]!.messages[0]!;
  assert.equal(msg.delivery, 'failed');
  assert.deepEqual(msg.file?.voice, voice);
};
