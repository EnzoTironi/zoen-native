// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  attachment,
  login,
  conversation,
  messagingCore,
  linked,
  vault,
  authenticate,
  socket,
  settle,
  mediaMessages,
} from '../transport-fixture.ts';
import { type MessagesState, type FileDoc, Messages } from '../messages.ts';
import { type CoreOperation } from '../worker-client.ts';
import assert from 'node:assert/strict';
import { type StoredSession } from '../session.ts';

export const scenario10 = async (t: TestContext) => {
  install(t);
  const file = {
    ...(await attachment()),
    path: 'VoiceNotes/native.webm',
    voice: {
      ms: 3400,
      levels: '08ff80',
      transcript: 'Bring the tent',
      reply: { to: 'source-entry', thread: true },
    },
  };
  const { voice: _, ...legacy } = file;
  const entry = {
    file_item_id: 'it_fixture',
    file_version: 1,
    id: 'voice',
    entry: 'ordered',
    author: login.identity,
    name: 'Member',
    text: '',
    at_ms: 1,
    delivery: 'confirmed',
    failure: null,
  };
  const received = [
    {
      ...conversation,
      messages: [
        { ...entry, file },
        { ...entry, id: 'legacy', file: legacy },
        { ...entry, id: 'sparse', file: { ...file, voice: { ms: 1000, reply: { to: 'quote' } } } },
      ],
    },
  ];
  const states: MessagesState[] = [];
  const queued: FileDoc[] = [];
  const codec = messagingCore();
  const core = {
    ...codec,
    text: async (op: CoreOperation) => {
      if (op.kind === 'restore') return JSON.stringify(received);
      if (op.kind === 'bootstrap' || op.kind === 'queue_file') {
        if (op.kind === 'queue_file') queued.push(JSON.parse(op.doc));
        return JSON.stringify({ snapshot: 'native voice', frames: [], conversations: received });
      }
      return codec.text(op);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const state = states.at(-1);
  if (state?.kind !== 'active') assert.fail('message controller failed');
  const files = state.conversations[0]!.messages.map((m) => m.file);
  assert.deepEqual(files[0], file);
  assert.deepEqual(files[1], { ...legacy, voice: null });
  assert.deepEqual(files[2]?.voice, {
    ms: 1000,
    levels: '',
    transcript: '',
    reply: { to: 'quote', thread: false },
  });
  await messages.forwardFile('sp_fixture', files[0]!);
  await messages.forwardFile('sp_fixture', files[1]!);
  assert.deepEqual(queued[0], { ...file, voice: { ...file.voice, reply: null } });
  assert.deepEqual(queued[1], files[1]);
  assert.deepEqual(files[0]?.voice?.reply, { to: 'source-entry', thread: true });
};

export const scenario12 = async (t: TestContext) => {
  install(t);
  const file = await attachment();
  const chunk = file.chunks[0]!;
  const entry = {
    file_item_id: 'it_fixture',
    file_version: 1,
    id: 'text',
    entry: 'ordered',
    author: login.identity,
    name: 'Member',
    text: 'Still readable',
    at_ms: 1,
    delivery: 'confirmed',
    failure: null,
    file: null,
  };
  const invalid = [
    { ...file, chunks: [{ ...chunk, blob: 'bad' }] },
    { ...file, chunks: [{ ...chunk, sha256: 'bad' }] },
    { ...file, chunks: [{ ...chunk, key: 'bad' }] },
    { ...file, thumb: { ...chunk, blob: 'bad' } },
  ];
  const received = [
    {
      ...conversation,
      messages: [
        ...invalid.map((file, i) => ({ ...entry, id: `invalid-${i}`, text: '', file })),
        { ...entry, id: 'valid', text: '', file },
        entry,
      ],
    },
    { ...conversation, id: 'sp_other', messages: [entry] },
  ];
  let saved: StoredSession = linked;
  const states: MessagesState[] = [];
  const core = messagingCore();
  const text = core.text;
  core.text = async (operation: CoreOperation) => {
    if (operation.kind === 'restore' && operation.snapshot === 'received attachments')
      return JSON.stringify(received);
    if (
      operation.kind === 'bootstrap' &&
      saved.kind === 'linked' &&
      saved.snapshot === 'received attachments'
    ) {
      return JSON.stringify({ snapshot: saved.snapshot, frames: [], conversations: received });
    }
    if (operation.kind === 'receive')
      return JSON.stringify({
        snapshot: 'received attachments',
        frames: [[9]],
        conversations: received,
      });
    return text(operation);
  };
  const open = (session: typeof linked) => {
    const messages = new Messages({
      core,
      session,
      vault: {
        ...vault,
        save: async (session) => {
          saved = session;
        },
      },
      changed: (state) => states.push(state),
    });
    t.after(() => messages.stop());
    return messages;
  };
  const check = () => {
    const state = states.at(-1);
    assert.equal(state?.kind, 'active');
    if (state?.kind !== 'active') assert.fail('message controller failed');
    const entries = state.conversations[0]!.messages;
    for (const [i] of invalid.entries()) {
      assert.equal(entries[i]!.id, `invalid-${i}`);
      assert.equal(entries[i]!.file, null);
      assert.equal(entries[i]!.text, 'Anexo indisponível');
    }
    assert.deepEqual(entries[invalid.length]!.file, file);
    assert.equal(entries.at(-1)!.text, entry.text);
    assert.equal(state.conversations[1]!.messages[0]!.text, entry.text);
    assert.equal(core.closed, false);
  };
  const messages = open(linked);
  await messages.start();
  await authenticate();
  socket().frame({ kind: 'event' });
  await settle();
  check();
  assert.equal(saved.kind === 'linked' && saved.snapshot, 'received attachments');
  assert.deepEqual(socket().sent.at(-1), new Uint8Array([9]));
  await messages.send({ space: 'sp_other', text: 'Can still send' });
  assert.deepEqual(socket().sent.at(-1), new Uint8Array([8]));
  // Save another received update, then restore that same unmodified snapshot.
  socket().frame({ kind: 'event' });
  await settle();
  check();
  messages.stop();
  assert.equal(saved.kind, 'linked');
  if (saved.kind !== 'linked') assert.fail('received snapshot was not persisted');
  const reloaded = open(saved);
  await reloaded.start();
  check();
  await authenticate();
  check();
  const state = states.at(-1);
  assert.equal(state?.kind === 'active' && state.connection.kind, 'ready');
  assert.equal(
    states.some((state) => state.kind === 'failed'),
    false,
  );
};

export const scenario13 = async (t: TestContext) => {
  const data = new Uint8Array([1, 2, 3]);
  const file = await attachment(data);
  let opened = data;
  const messages = mediaMessages(t, async () => opened);
  t.mock.method(globalThis, 'fetch', async () => new Response(new Uint8Array(file.bytes + 40)));
  assert.deepEqual(new Uint8Array(await (await messages.openFile(file)).arrayBuffer()), data);
  await assert.rejects(messages.openFile({ ...file, sha256: '00'.repeat(32) }));
  for (const length of [2, 4]) {
    opened = new Uint8Array(length);
    await assert.rejects(messages.openFile(file));
  }
  opened = data;
  const thumb = { ...file, thumb: file.chunks[0]! };
  assert.equal((await messages.openFile(thumb, true)).type, 'image/png');
  await assert.rejects(
    messages.openFile({ ...thumb, thumb: { ...thumb.thumb, sha256: '00'.repeat(32) } }, true),
  );
};

export const scenario14 = async (t: TestContext) => {
  const file = await attachment();
  const messages = mediaMessages(t, async () => {
    assert.fail('worker called for invalid descriptor');
  });
  const fetching = t.mock.method(globalThis, 'fetch', async () => {
    assert.fail('fetch called for invalid descriptor');
  });
  const chunk = file.chunks[0]!;
  const invalid = [
    { ...file, bytes: 0 },
    { ...file, bytes: (512 << 20) + 1 },
    { ...file, sha256: 'bad' },
    { ...file, chunks: [] },
    { ...file, chunks: Array(514).fill(chunk), bytes: 3 * 514 },
    { ...file, chunks: [{ ...chunk, bytes: 0 }] },
    { ...file, chunks: [{ ...chunk, bytes: (16 << 20) + 1 }], bytes: (16 << 20) + 1 },
    { ...file, chunks: Array(100).fill({ ...chunk, bytes: 16 << 20 }), bytes: 100 * (16 << 20) },
    { ...file, bytes: file.bytes + 1 },
    { ...file, name: 'é'.repeat(128 << 10) },
    { ...file, thumb: { ...chunk, bytes: (2 << 20) + 1 } },
  ];
  for (const doc of invalid) {
    await assert.rejects(messages.openFile(doc));
    await assert.rejects(messages.openFile(doc, true));
  }
  assert.equal(fetching.mock.callCount(), 0);
};

export const scenario15 = async (t: TestContext) => {
  const file = await attachment();
  const messages = mediaMessages(t, async () => {
    assert.fail('invalid ciphertext reached worker');
  });
  t.mock.method(globalThis, 'fetch', async () => new Response(new Uint8Array(file.bytes + 39)));
  await assert.rejects(messages.openFile(file));
  let cancelled = false;
  t.mock.method(
    globalThis,
    'fetch',
    async () =>
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(new Uint8Array(file.bytes + 40));
            controller.enqueue(new Uint8Array(1));
          },
          cancel() {
            cancelled = true;
          },
        }),
      ),
  );
  await assert.rejects(messages.openFile(file));
  assert.equal(cancelled, true);
};

export const scenario16 = async (t: TestContext) => {
  const plain = new Uint8Array([1, 2, 3]);
  const file = await attachment(plain);
  const sealed = new Uint8Array(file.bytes + 40);
  const messages = mediaMessages(t, async (op) => {
    if (op.kind === 'local_chunk') {
      assert.equal(op.blob, file.chunks[0]!.blob);
      return sealed;
    }
    if (op.kind === 'open_chunk') {
      assert.deepEqual(op.sealed, sealed);
      return plain;
    }
    throw new Error('Unexpected operation');
  });
  t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 404 }));
  assert.deepEqual(new Uint8Array(await (await messages.openFile(file)).arrayBuffer()), plain);
};

export const scenario17 = async (t: TestContext) => {
  const first = await attachment(new Uint8Array([1, 2]));
  const last = await attachment(new Uint8Array([3]));
  const file = await attachment();
  file.chunks = [...first.chunks, ...last.chunks];
  const messages = mediaMessages(t, async (op) => {
    if (op.kind !== 'open_chunk') throw new Error('Unexpected test operation.');
    return new Uint8Array(JSON.parse(op.chunk).bytes === 2 ? [1, 2] : [3]);
  });
  let calls = 0;
  t.mock.method(globalThis, 'fetch', async () => {
    file.bytes = 0;
    file.sha256 = '00'.repeat(32);
    file.chunks.length = 0;
    return new Response(new Uint8Array((calls++ ? 1 : 2) + 40));
  });
  assert.deepEqual(
    new Uint8Array(await (await messages.openFile(file)).arrayBuffer()),
    new Uint8Array([1, 2, 3]),
  );
  assert.equal(calls, 2);
};
