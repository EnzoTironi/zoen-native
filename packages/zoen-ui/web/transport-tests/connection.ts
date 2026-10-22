// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  unitCodec,
  vault,
  login,
  authenticate,
  socket,
  settle,
  Socket,
  challenge,
  ready,
  messagingCore,
  linked,
} from '../transport-fixture.ts';
import { type ConnectionState, AuthenticatedTransport } from '../transport.ts';
import assert from 'node:assert/strict';
import { type CoreOperation } from '../worker-client.ts';
import { type StoredSession } from '../session.ts';
import { Messages } from '../messages.ts';

export const scenario01 = async (t: TestContext) => {
  install(t);
  const states: ConnectionState[] = [];
  const transport = new AuthenticatedTransport(
    unitCodec(),
    vault,
    login,
    (state) => states.push(state),
    async (frame) => {
      if (frame.kind === 'ready') await transport.request('fetch_link', 'box');
    },
  );
  t.after(() => transport.stop());
  transport.start();
  await authenticate();
  const first = socket();
  assert.equal(first.sent.length, 3);
  first.close();
  await settle();
  assert.equal(states.at(-1)?.kind, 'reconnecting');
  assert.equal(
    states.some((state) => state.kind === 'failed'),
    false,
  );
  t.mock.timers.tick(900);
  await settle();
  assert.equal(Socket.instances.length, 2);
  assert.notEqual(socket(), first);
  await authenticate();
  assert.equal(states.at(-1)?.kind, 'ready');
};

export const scenario02 = async (t: TestContext) => {
  install(t);
  const codec = unitCodec();
  let finish = (_: string) => {};
  const delayed = new Promise<string>((resolve) => {
    finish = resolve;
  });
  let delay = true;
  const core = {
    ...codec,
    text: async (operation: CoreOperation) => {
      const result = await codec.text(operation);
      if (delay && JSON.parse(result).kind === 'ready') {
        delay = false;
        return delayed;
      }
      return result;
    },
  };
  const states: ConnectionState[] = [];
  let received = 0;
  const transport = new AuthenticatedTransport(
    core,
    vault,
    login,
    (state) => states.push(state),
    async () => {
      ++received;
    },
  );
  t.after(() => transport.stop());
  transport.start();
  await authenticate();
  socket().close();
  t.mock.timers.tick(900);
  await settle();
  socket().open();
  await settle();
  socket().frame(challenge);
  await settle();
  finish(JSON.stringify(ready));
  await settle();
  assert.equal(received, 0);
  assert.equal(states.at(-1)?.kind, 'authenticating');
  socket().frame(ready);
  await settle();
  assert.equal(received, 1);
  assert.equal(states.at(-1)?.kind, 'ready');
};

export const scenario03 = async (t: TestContext) => {
  install(t);
  const codec = unitCodec();
  let finish = (_: Uint8Array<ArrayBuffer>) => {};
  const delayed = new Promise<Uint8Array<ArrayBuffer>>((resolve) => {
    finish = resolve;
  });
  const core = {
    ...codec,
    bytes: async (operation: CoreOperation) =>
      operation.kind === 'auth' ? delayed : codec.bytes(operation),
  };
  const transport = new AuthenticatedTransport(
    core,
    vault,
    login,
    () => {},
    async () => {},
  );
  t.after(() => transport.stop());
  transport.start();
  socket().open();
  await settle();
  socket().frame(challenge);
  await settle();
  const first = socket();
  first.close();
  t.mock.timers.tick(900);
  await settle();
  finish(new Uint8Array([99]));
  await settle();
  assert.equal(first.sent.length, 1);
  assert.equal(socket().sent.length, 0);
};

export const scenario04 = async (t: TestContext) => {
  install(t);
  const states: ConnectionState[] = [];
  const transport = new AuthenticatedTransport(
    unitCodec(),
    vault,
    login,
    (state) => states.push(state),
    async () => {},
  );
  t.after(() => transport.stop());
  transport.start();
  socket().open();
  await settle();
  socket().frame(challenge);
  await settle();
  socket().frame({ kind: 'other' });
  await settle();
  assert.equal(states.at(-1)?.kind, 'authenticating');
  socket().frame(ready);
  await settle();
  assert.equal(states.at(-1)?.kind, 'ready');
};

export const scenario09 = async (t: TestContext) => {
  install(t);
  const core = messagingCore();
  const saves: StoredSession[] = [];
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        saves.push(session);
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const first = socket();
  first.close();
  await settle();
  await messages.send({ space: 'sp_fixture', text: 'Keep this offline' });
  assert.equal(first.sent.length, 3);
  assert.equal(saves.at(-1)?.kind, 'linked');
  assert.equal(
    (saves.at(-1) as Extract<StoredSession, { kind: 'linked' }>).snapshot,
    'new MLS send ratchet',
  );
  t.mock.timers.tick(900);
  await settle();
  await authenticate();
  assert.equal(socket().sent.length, 3);
  assert.equal(core.closed, false);
};

export const scenario23 = async (t: TestContext) => {
  install(t);
  const codec = unitCodec();
  let release = (_: Uint8Array) => {};
  const delayed = new Promise<Uint8Array>((resolve) => {
    release = resolve;
  });
  const core = {
    ...codec,
    bytes: async (operation: CoreOperation) =>
      operation.kind === 'request' ? delayed : codec.bytes(operation),
  };
  const transport = new AuthenticatedTransport(
    core,
    vault,
    login,
    () => {},
    async () => {},
  );
  t.after(() => transport.stop());
  transport.start();
  await authenticate();
  const old = socket();
  const asking = transport.request('agent_status', 'agent');
  const rejected = assert.rejects(asking, /conexão mudou/);
  old.close();
  t.mock.timers.tick(900);
  await settle();
  await authenticate();
  const replacement = socket();
  const before = replacement.sent.length;
  release(new Uint8Array([99]));
  await rejected;
  assert.equal(replacement.sent.length, before);
};

export const scenario26 = async (t: TestContext) => {
  install(t);
  const transport = new AuthenticatedTransport(
    unitCodec(),
    vault,
    login,
    () => {},
    async () => {},
  );
  t.after(() => transport.stop());
  transport.start();
  socket().open();
  await settle();
  socket().frame({
    ...challenge,
    capabilities: ['key-package-claim-receipts'],
    server_time_ms: '1770000000123',
  });
  await settle();
  socket().frame(ready);
  await settle();
  assert.equal(transport.claimClock, '1770000000123');
  socket().close();
  t.mock.timers.tick(900);
  await settle();
  await authenticate();
  assert.equal(transport.claimClock, '');
};
