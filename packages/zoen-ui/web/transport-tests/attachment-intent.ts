// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  messagingCore,
  linked,
  vault,
  authenticate,
  conversation,
  socket,
  settle,
  stagedAttachmentCore,
  exactAttachment,
} from '../transport-fixture.ts';
import { type CoreOperation } from '../worker-client.ts';
import { Messages } from '../messages.ts';
import assert from 'node:assert/strict';

export const scenario32 = async (t: TestContext) => {
  install(t);
  const codec = messagingCore();
  const requests: unknown[] = [];
  const core = {
    ...codec,
    text: async (op: CoreOperation) => {
      if (op.kind !== 'attachment_producer') return codec.text(op);
      const request = JSON.parse(op.request);
      requests.push(request);
      return request.op === 'begin'
        ? 'attachment_owner'
        : request.op === 'issue'
          ? 'attachment_owner_7'
          : '';
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const mutation = await messages.newAttachmentMutationId();
  await messages.releaseAttachmentMutation(mutation, true);
  await messages.releaseAttachmentMutation(mutation, false);
  assert.deepEqual(requests, [
    { op: 'begin' },
    { op: 'issue', producer_id: 'attachment_owner' },
    { op: 'accepted', mutation_id: 'attachment_owner_7' },
    { op: 'drop', mutation_id: 'attachment_owner_7' },
  ]);
  messages.stop();
  await assert.rejects(messages.newAttachmentMutationId(), /closed/);
  assert.equal(requests.length, 4);
};

export const scenario33 = async (t: TestContext) => {
  install(t);
  const codec = messagingCore();
  const sealed: Uint8Array[] = [];
  const plaintext: Uint8Array[] = [];
  const requests: Extract<CoreOperation, { kind: 'queue_attachment_batch' }>[] = [];
  let releaseSeal = () => {};
  const sealing = new Promise<void>((resolve) => {
    releaseSeal = resolve;
  });
  let releaseSave = () => {};
  const saving = new Promise<void>((resolve) => {
    releaseSave = resolve;
  });
  const core = {
    ...codec,
    bytes: async (operation: CoreOperation) => {
      if (operation.kind !== 'seal_file') return codec.bytes(operation);
      plaintext.push(Uint8Array.from(operation.bytes));
      if (plaintext.length === 1) await sealing;
      const frame = new Uint8Array([1, 2, 3, plaintext.length]);
      sealed.push(frame);
      return frame;
    },
    text: async (operation: CoreOperation) => {
      if (operation.kind !== 'queue_attachment_batch') return codec.text(operation);
      requests.push(operation);
      return JSON.stringify({
        result: [
          { itemId: 'it_a', messageId: 'msg_a' },
          { itemId: 'it_b', messageId: 'msg_b' },
        ],
        update: {
          snapshot: 'retained batch',
          frames: [[71], [72], [73], [74]],
          conversations: [conversation],
        },
      });
    },
  };
  let saves = 0;
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind === 'linked' && session.snapshot === 'retained batch') {
          saves++;
          await saving;
        }
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const sent = socket().sent.length;
  const options: { presentation: 'media' | 'file'; grouping: 'grouped' | 'separate' } = {
    presentation: 'file',
    grouping: 'grouped',
  };
  const files = [
    {
      name: 'a.jpg',
      mime: 'image/jpeg',
      bytes: new Uint8Array([1]),
      mutationId: 'owner_1',
      caption: '  First  ',
    },
    {
      name: 'b.jpg',
      mime: 'image/jpeg',
      bytes: new Uint8Array([2]),
      mutationId: 'owner_2',
      caption: 'Second\n',
    },
  ];
  const sending = messages.sendFiles('sp_fixture', files, {
    replyTo: 'root',
    thread: true,
    options,
  });
  options.presentation = 'media';
  options.grouping = 'separate';
  files[1]!.caption = 'late';
  files[1]!.bytes[0] = 99;
  releaseSeal();
  await settle();
  assert.equal(saves, 1);
  assert.equal(socket().sent.length, sent, 'No publication can precede the complete durable save');
  assert.deepEqual(plaintext, [new Uint8Array([1]), new Uint8Array([2])]);
  assert.equal(requests.length, 1);
  assert.deepEqual(JSON.parse(requests[0]!.request), {
    space_id: 'sp_fixture',
    reply_to: 'root',
    thread: true,
    options: { presentation: 'file', grouping: 'grouped' },
    items: [
      { mutation_id: 'owner_1', caption: '  First  ' },
      { mutation_id: 'owner_2', caption: 'Second\n' },
    ],
  });
  const framed = requests[0]!.framed;
  const view = new DataView(framed.buffer, framed.byteOffset, framed.byteLength);
  assert.equal(view.getUint32(0, true), sealed[0]!.length);
  assert.deepEqual(framed.slice(4, 8), sealed[0]);
  assert.equal(view.getUint32(8, true), sealed[1]!.length);
  assert.deepEqual(framed.slice(12), sealed[1]);
  releaseSave();
  assert.deepEqual(await sending, [
    { itemId: 'it_a', messageId: 'msg_a' },
    { itemId: 'it_b', messageId: 'msg_b' },
  ]);
  assert.equal(socket().sent.length, sent + 4);
};

export const scenario34 = async (t: TestContext) => {
  install(t);
  const codec = messagingCore();
  let seals = 0,
    queues = 0;
  const core = {
    ...codec,
    bytes: async (operation: CoreOperation) => {
      if (operation.kind !== 'seal_file') return codec.bytes(operation);
      if (++seals === 2) throw Error('Encoder failed');
      return new Uint8Array([1, 2, 3, 4]);
    },
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'queue_attachment_batch') queues++;
      return codec.text(operation);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const files = [1, 2].map((index) => ({
    name: `${index}.jpg`,
    mime: 'image/jpeg',
    bytes: new Uint8Array([index]),
    mutationId: `owner_${index}`,
    caption: '',
  }));
  await assert.rejects(
    messages.sendFiles('sp_fixture', files, {
      replyTo: null,
      thread: false,
      options: { presentation: 'media', grouping: 'separate', quality: 'original' } as never,
    }),
  );
  assert.equal(seals, 0);
  await assert.rejects(
    messages.sendFiles('sp_fixture', files, {
      replyTo: null,
      thread: false,
      options: { presentation: 'media', grouping: 'grouped' },
    }),
    /preparar/,
  );
  assert.equal(seals, 2);
  assert.equal(queues, 0, 'A failed second file cannot queue the first file');
};

export const scenario35 = async (t: TestContext) => {
  install(t);
  const core = stagedAttachmentCore();
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const order: string[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind !== 'linked') assert.fail('linked snapshot required');
        if (session.snapshot === 'retained attachment') {
          order.push('save started');
          await held;
          order.push('saved intent');
        }
        if (session.snapshot === 'uploaded attachment') order.push('saved upload');
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  const puts = t.mock.method(globalThis, 'fetch', async () => {
    order.push('PUT');
    return new Response(null, { status: 200 });
  });
  await messages.start();
  await authenticate();
  const before = socket().sent.length;
  const sending = messages.sendFile('sp_fixture', {
    name: 'report.txt',
    mime: 'text/plain',
    bytes: new Uint8Array([1]),
    attachment: exactAttachment,
  });
  await settle();
  assert.deepEqual(order, ['save started']);
  assert.equal(puts.mock.callCount(), 0);
  assert.equal(socket().sent.length, before);
  release();
  assert.deepEqual(await sending, { itemId: 'it_same', messageId: 'msg_same' });
  assert.deepEqual(order, ['save started', 'saved intent', 'PUT', 'saved upload']);
  assert.deepEqual(
    socket()
      .sent.slice(before)
      .map((frame) => Array.from(frame)),
    [[61], [62]],
  );
  assert.deepEqual(JSON.parse(core.requests[0]!), {
    space_id: 'sp_fixture',
    mutation_id: exactAttachment.mutationId,
    caption: exactAttachment.caption,
    reply_to: 'root-hash',
    thread: true,
  });
};

export const scenario36 = async (t: TestContext) => {
  install(t);
  const core = stagedAttachmentCore();
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind === 'linked' && session.snapshot === 'retained attachment')
          throw Error('QuotaExceededError');
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  const puts = t.mock.method(globalThis, 'fetch', async () => {
    assert.fail('upload before durability');
  });
  await messages.start();
  await authenticate();
  const before = socket().sent.length;
  await assert.rejects(
    messages.sendFile('sp_fixture', {
      name: 'report.txt',
      mime: 'text/plain',
      bytes: new Uint8Array([1]),
      attachment: exactAttachment,
    }),
    /QuotaExceededError/,
  );
  assert.equal(core.closed, true);
  assert.equal(puts.mock.callCount(), 0);
  assert.equal(socket().sent.length, before);
};

export const scenario37 = async (t: TestContext) => {
  install(t);
  const core = stagedAttachmentCore();
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let retained = false;
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind === 'linked' && session.snapshot === 'retained attachment') {
          await held;
          retained = true;
        }
      },
    },
    changed: () => {},
  });
  const puts = t.mock.method(globalThis, 'fetch', async () => {
    assert.fail('retired owner cannot upload');
  });
  await messages.start();
  await authenticate();
  const before = socket().sent.length;
  const sending = messages.sendFile('sp_fixture', {
    name: 'report.txt',
    mime: 'text/plain',
    bytes: new Uint8Array([1]),
    attachment: exactAttachment,
  });
  await settle();
  messages.stop();
  release();
  await sending;
  assert.equal(retained, true);
  assert.equal(puts.mock.callCount(), 0);
  assert.equal(socket().sent.length, before);
};

export const scenario38 = async (t: TestContext) => {
  install(t);
  const codec = messagingCore();
  let releaseSave = () => {},
    releaseBootstrap = () => {};
  const heldSave = new Promise<void>((resolve) => {
    releaseSave = resolve;
  });
  const heldBootstrap = new Promise<void>((resolve) => {
    releaseBootstrap = resolve;
  });
  const order: string[] = [];
  let bootstraps = 0,
    staged = false,
    uploaded = false;
  const core = {
    ...codec,
    bytes: async (op: CoreOperation) => {
      if (op.kind === 'seal_file') return new Uint8Array([1, 2, 3]);
      if (op.kind === 'local_chunk') {
        order.push('local chunk');
        return new Uint8Array([9]);
      }
      return codec.bytes(op);
    },
    text: async (op: CoreOperation) => {
      if (op.kind === 'queue_attachment') {
        staged = true;
        return JSON.stringify({
          result: { itemId: 'it_retained', messageId: 'msg_retained' },
          update: {
            snapshot: 'retained across reconnect',
            frames: [],
            conversations: [conversation],
          },
        });
      }
      if (op.kind === 'bootstrap' && ++bootstraps === 2) {
        order.push('replacement bootstrap started');
        await heldBootstrap;
        return JSON.stringify({
          snapshot: 'replacement bootstrap',
          frames: [],
          conversations: [conversation],
        });
      }
      if (op.kind === 'document_uploads') {
        order.push('document uploads');
        return JSON.stringify(staged && !uploaded ? ['ab'.repeat(32)] : []);
      }
      if (op.kind === 'document_uploaded') {
        order.push('upload acknowledged');
        uploaded = true;
        return JSON.stringify({
          snapshot: 'uploaded after bootstrap',
          frames: [[63]],
          conversations: [conversation],
        });
      }
      return codec.text(op);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind !== 'linked') assert.fail('linked snapshot required');
        if (session.snapshot === 'retained across reconnect') {
          order.push('attachment save started');
          await heldSave;
          order.push('attachment saved');
        }
        if (session.snapshot === 'replacement bootstrap') order.push('replacement bootstrap saved');
        if (session.snapshot === 'uploaded after bootstrap') order.push('upload saved');
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  const put = t.mock.method(globalThis, 'fetch', async () => {
    order.push('PUT');
    return new Response(null, { status: 200 });
  });
  await messages.start();
  await authenticate();
  order.length = 0;
  const first = socket();
  const sending = messages.sendFile('sp_fixture', {
    name: 'report.txt',
    mime: 'text/plain',
    bytes: new Uint8Array([1]),
    attachment: exactAttachment,
  });
  await settle();
  assert.deepEqual(order, ['attachment save started']);
  first.close();
  t.mock.timers.tick(900);
  await settle();
  await authenticate();
  const replacement = socket();
  assert.notEqual(replacement, first);
  assert.equal(bootstraps, 1, 'new Ready bootstrap must still wait behind the held send');
  assert.equal(replacement.sent.length, 2, 'only hello/auth precedes the durable save');
  releaseSave();
  assert.deepEqual(await sending, { itemId: 'it_retained', messageId: 'msg_retained' });
  await settle();
  assert.deepEqual(order, [
    'attachment save started',
    'attachment saved',
    'replacement bootstrap started',
  ]);
  assert.equal(put.mock.callCount(), 0, 'old send cannot pump under the new ready generation');
  assert.equal(replacement.sent.length, 2);
  releaseBootstrap();
  await settle();
  await settle();
  assert.deepEqual(order, [
    'attachment save started',
    'attachment saved',
    'replacement bootstrap started',
    'replacement bootstrap saved',
    'document uploads',
    'local chunk',
    'PUT',
    'upload acknowledged',
    'upload saved',
  ]);
  assert.equal(put.mock.callCount(), 1);
  assert.deepEqual(Array.from(replacement.sent.at(-1)!), [63]);
};
