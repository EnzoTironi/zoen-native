// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  messagingCore,
  linked,
  vault,
  authenticate,
  socket,
  settle,
  Socket,
  conversation,
} from '../transport-fixture.ts';
import { type MessagesState, Messages } from '../messages.ts';
import assert from 'node:assert/strict';
import { type CoreOperation } from '../worker-client.ts';
import { type StoredSession } from '../session.ts';

export const scenario05 = async (t: TestContext) => {
  install(t);
  const core = messagingCore();
  const states: MessagesState[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async () => {
        throw new Error('QuotaExceededError');
      },
    },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  assert.equal(socket().sent.length, 2); // hello/auth only; no MLS/bootstrap frame.
  assert.equal(core.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  t.mock.timers.tick(40_000);
  await settle();
  assert.equal(Socket.instances.length, 1);
};

export const scenario06 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  let refusal = 'Only the person who sent this message can change it.';
  let mutationSaves = 0;
  const core = {
    ...base,
    text: async (operation: CoreOperation) => {
      if (operation.kind !== 'queue_annotation') return base.text(operation);
      if (refusal) throw new Error(refusal);
      return JSON.stringify({ snapshot: 'mutation', frames: [[9]], conversations: [conversation] });
    },
  };
  const states: MessagesState[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (saved) => {
        if (saved.kind === 'linked' && saved.snapshot === 'mutation') {
          assert.equal(socket().sent.length, 3);
          ++mutationSaves;
        }
      },
    },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  await assert.rejects(
    messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } }),
    /Só quem enviou/,
  );
  assert.equal(states.at(-1)?.kind, 'active');
  assert.equal(base.closed, false);
  assert.equal(socket().sent.length, 3);
  refusal = 'Essa mensagem foi apagada.';
  await assert.rejects(
    messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } }),
    /Essa mensagem foi apagada/,
  );
  refusal = 'Write a message of up to 32 KB.';
  await assert.rejects(
    messages.annotate('sp_fixture', 'target', { Edit: { text: '' } }),
    /não pode ficar vazia/,
  );
  assert.equal(states.at(-1)?.kind, 'active');
  refusal = '';
  await messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } });
  assert.equal(mutationSaves, 1);
  assert.equal(socket().sent.length, 4);
};

export const scenario07 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  const core = {
    ...base,
    text: async (operation: CoreOperation) =>
      operation.kind === 'queue_annotation'
        ? JSON.stringify({ snapshot: 'mutation', frames: [[9]], conversations: [conversation] })
        : base.text(operation),
  };
  const states: MessagesState[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (saved) => {
        if (saved.kind === 'linked' && saved.snapshot === 'mutation')
          throw new Error('Storage denied');
      },
    },
    changed: (state) => states.push(state),
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const current = socket();
  await assert.rejects(messages.annotate('sp_fixture', 'target', 'Delete'), /Storage denied/);
  assert.equal(base.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  assert.equal(current.sent.length, 3);
};

export const scenario08 = async (t: TestContext) => {
  install(t);
  const core = messagingCore();
  let complete = () => {};
  const saving = new Promise<void>((resolve) => {
    complete = resolve;
  });
  const saves: StoredSession[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        saves.push(session);
        if (session.kind === 'linked' && session.snapshot === 'new MLS send ratchet') await saving;
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const first = socket();
  assert.equal(first.sent.length, 3);
  const sending = messages.send({ space: 'sp_fixture', text: 'Encrypted message' });
  await settle();
  assert.equal(first.sent.length, 3);
  first.close();
  await settle();
  complete();
  await sending;
  assert.equal(first.sent.length, 3);
  t.mock.timers.tick(900);
  await settle();
  await authenticate();
  assert.equal(socket().sent.length, 3);
  assert.equal(saves.length, 3);
  assert.equal(core.closed, false);
};

export const scenario31 = async (t: TestContext) => {
  install(t);
  const core = messagingCore();
  let release = () => {};
  const saving = new Promise<void>((resolve) => {
    release = resolve;
  });
  const saves: StoredSession[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        saves.push(session);
        if (session.kind === 'linked' && session.snapshot === 'new MLS send ratchet') await saving;
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const first = socket();
  const sending = messages.send({ space: 'sp_fixture', text: 'Keep the encrypted outbox' });
  await settle();
  // Native socket state can change while encryption/storage awaits, before the
  // browser has dispatched the onclose callback that updates connection state.
  first.readyState = 2;
  release();
  await sending;
  assert.equal(first.sent.length, 3);
  assert.equal(core.closed, false);
  assert.equal(
    (saves.at(-1) as Extract<StoredSession, { kind: 'linked' }>).snapshot,
    'new MLS send ratchet',
  );
  t.mock.timers.tick(900);
  await settle();
  await authenticate();
  assert.notEqual(socket(), first);
  assert.equal(socket().sent.length, 3);
  assert.equal(core.closed, false);
};
