// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  messagingCore,
  conversation,
  linked,
  vault,
  authenticate,
  socket,
  settle,
} from './fixture.ts';
import { type CoreOperation } from '../worker-client.ts';
import { Messages, type MessagesState } from '../messages.ts';
import assert from 'node:assert/strict';

export const scenario21 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  let queued = false;
  let uploaded = false;
  let release = () => {};
  const durable = new Promise<void>((resolve) => {
    release = resolve;
  });
  const core = {
    ...base,
    bytes: async (op: CoreOperation) =>
      op.kind === 'local_chunk' ? new Uint8Array([1, 2, 3]) : base.bytes(op),
    text: async (op: CoreOperation) => {
      if (op.kind === 'document_request') {
        queued = true;
        return JSON.stringify({
          result: true,
          update: { snapshot: 'document queued', frames: [], conversations: [conversation] },
          uploads: ['blob'],
          downloads: [],
        });
      }
      if (op.kind === 'document_uploads')
        return JSON.stringify(queued && !uploaded ? ['blob'] : []);
      if (op.kind === 'document_uploaded') {
        uploaded = true;
        return JSON.stringify({
          snapshot: 'document uploaded',
          frames: [[19]],
          conversations: [conversation],
        });
      }
      return base.text(op);
    },
  };
  const saved: string[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind !== 'linked') assert.fail('expected linked session');
        saved.push(session.snapshot);
        if (session.snapshot === 'document queued') await durable;
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  const fetching = t.mock.method(
    globalThis,
    'fetch',
    async () => new Response(null, { status: 200 }),
  );
  await messages.start();
  await authenticate();
  const before = socket().sent.length;
  const commit = messages.documentRequest({
    op: 'page_commit',
    item_id: 'it_page',
    note: 'Editou',
  });
  await settle();
  assert.equal(fetching.mock.callCount(), 0);
  assert.equal(socket().sent.length, before);
  release();
  assert.equal(await commit, true);
  assert.equal(fetching.mock.callCount(), 1);
  assert.deepEqual(saved.slice(-2), ['document queued', 'document uploaded']);
  assert.deepEqual(socket().sent.at(-1), new Uint8Array([19]));
};

export const scenario22 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  let queued = false;
  const core = {
    ...base,
    bytes: async (op: CoreOperation) =>
      op.kind === 'local_chunk' ? new Uint8Array([1]) : base.bytes(op),
    text: async (op: CoreOperation) => {
      if (op.kind === 'document_request') {
        queued = true;
        return JSON.stringify({
          result: true,
          update: { snapshot: 'document queued', frames: [], conversations: [conversation] },
          uploads: ['blob'],
          downloads: [],
        });
      }
      if (op.kind === 'document_uploads') return JSON.stringify(queued ? ['blob'] : []);
      if (op.kind === 'document_uploaded')
        return JSON.stringify({
          snapshot: 'document uploaded',
          frames: [[20]],
          conversations: [conversation],
        });
      return base.text(op);
    },
  };
  const states: MessagesState[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind === 'linked' && session.snapshot === 'document uploaded')
          throw new Error('QuotaExceededError');
      },
    },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 200 }));
  await messages.start();
  await authenticate();
  const before = socket().sent.length;
  await assert.rejects(
    messages.documentRequest({ op: 'page_commit', item_id: 'it_page', note: 'Editou' }),
    /QuotaExceededError/,
  );
  assert.equal(base.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  assert.equal(socket().sent.length, before);
};
