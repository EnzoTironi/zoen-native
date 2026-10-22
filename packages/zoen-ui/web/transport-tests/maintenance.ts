// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  messagingCore,
  linked,
  vault,
  authenticate,
  socket,
  login,
  settle,
  conversation,
} from './fixture.ts';
import { type CoreOperation } from '../worker-client.ts';
import { Messages } from '../messages.ts';
import assert from 'node:assert/strict';

export const scenario24 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  let saves = 0;
  const events: unknown[] = [];
  const typed: unknown[] = [];
  const core = {
    ...base,
    bytes: async (operation: CoreOperation) => {
      if (operation.kind === 'typing_frame') {
        typed.push(operation);
        return new Uint8Array([31]);
      }
      return base.bytes(operation);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async () => {
        saves++;
      },
    },
    changed: () => {},
    ephemeral: (event) => events.push(event),
  });
  t.after(() => messages.stop());
  await messages.start();
  await messages.typing('sp_fixture', true);
  assert.equal(typed.length, 0);
  await authenticate();
  const before = saves;
  socket().frame({
    kind: 'ephemeral',
    space: 'sp_fixture',
    from: login.identity,
    event: 'typing',
    detail: '',
  });
  await settle();
  socket().frame({ kind: 'presence', identity: login.identity, online: true });
  await settle();
  assert.equal(events.length, 2);
  assert.equal(saves, before);
  await messages.typing('sp_fixture', true);
  assert.deepEqual(typed, [{ kind: 'typing_frame', space: 'sp_fixture', typing: true }]);
  assert.deepEqual(socket().sent.at(-1), new Uint8Array([31]));
};

export const scenario27 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  let ticks = 0;
  let finish = () => {};
  const saving = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const core = {
    ...base,
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'bootstrap')
        return JSON.stringify({
          snapshot: 'with timer',
          frames: [],
          conversations: [conversation],
          maintenance_ms: 1000,
        });
      if (operation.kind === 'maintenance') {
        ticks++;
        return JSON.stringify({
          snapshot: 'staged membership',
          frames: [[88]],
          conversations: [conversation],
          maintenance_ms: 1000,
        });
      }
      return base.text(operation);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (saved) => {
        if (saved.kind === 'linked' && saved.snapshot === 'staged membership') await saving;
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  t.mock.timers.tick(1000);
  await settle();
  assert.equal(ticks, 1);
  assert.equal(socket().sent.length, 2);
  finish();
  await settle();
  assert.equal(socket().sent.at(-1)?.[0], 88);
  socket().close();
  t.mock.timers.tick(1000);
  await settle();
  assert.equal(ticks, 1);
};
