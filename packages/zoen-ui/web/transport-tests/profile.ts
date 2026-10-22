// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  messagingCore,
  linked,
  login,
  conversation,
  vault,
  authenticate,
  settle,
  socket,
} from './fixture.ts';
import { type CoreOperation } from '../worker-client.ts';
import { Messages } from '../messages.ts';
import assert from 'node:assert/strict';

export const scenario25 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  const session = structuredClone(linked);
  const requests: Extract<CoreOperation, { kind: 'request' }>[] = [];
  let confirmations = 0;
  const saved: string[] = [];
  const profile = {
    id: login.identity,
    kind: 'Person',
    name: '',
    bio: '',
    handle: 'new_alias',
    tint_hex: '#888888',
    glyph: null,
    owner: null,
  };
  const core = {
    ...base,
    bytes: async (operation: CoreOperation) => {
      if (operation.kind === 'request') requests.push(operation);
      return base.bytes(operation);
    },
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'handle_profile') return JSON.stringify(profile);
      if (operation.kind === 'handle_confirmed') {
        confirmations++;
        return JSON.stringify({
          snapshot: 'alias confirmed',
          frames: [],
          conversations: [conversation],
        });
      }
      return base.text(operation);
    },
  };
  const messages = new Messages({
    core,
    session,
    vault: {
      ...vault,
      save: async (stored) => {
        if (stored.kind === 'linked') saved.push(stored.account.handle);
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  const old = session.account.handle;
  const rejected = assert.rejects(messages.updateHandle('@New_Alias'), /já está em uso/);
  await settle();
  assert.equal(confirmations, 0);
  assert.equal(session.account.handle, old);
  socket().frame({ kind: 'request_error', id: requests[0]!.requestId, message: 'handle_taken' });
  await rejected;
  assert.equal(confirmations, 0);
  assert.equal(session.account.handle, old);
  assert.equal(base.closed, false);
  const changing = messages.updateHandle('@New_Alias');
  await settle();
  assert.equal(confirmations, 0);
  socket().frame({
    kind: 'response',
    id: requests[1]!.requestId,
    value: { kind: 'registered', profile },
  });
  assert.equal(await changing, 'new_alias');
  assert.equal(confirmations, 1);
  assert.equal(session.account.handle, 'new_alias');
  assert.equal(saved.at(-1), 'new_alias');
};
