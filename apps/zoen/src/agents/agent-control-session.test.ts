import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentControlSession, type AgentControlPort } from './agent-control-session.ts';
import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';

const first: AccountScope = { identity: 'a', device: 'd', relay: 'r', epoch: 1 };
const second: AccountScope = { ...first, epoch: 2 };

test('old reads cannot cross an account epoch', async () => {
  let scope = first;
  let resolve!: (status: 'Active') => void;
  const port: AgentControlPort = {
    scope: () => scope,
    read: () =>
      new Promise((done) => {
        resolve = done;
      }),
    control: async () => {},
  };
  const session = new AgentControlSession(port, 'agent');
  session.activate(first);
  const read = session.refresh();
  scope = second;
  session.activate(second);
  resolve('Active');
  await read;
  assert.equal(session.getSnapshot().scopeKey, accountScopeKey(second));
  assert.equal(session.getSnapshot().status, null);
  assert.equal(session.ticket(), null);
});

test('unknown delivery stays pending without replay until target', async () => {
  let status: 'Active' | 'Paused' = 'Active';
  let calls = 0;
  const port: AgentControlPort = {
    scope: () => first,
    read: async () => status,
    control: async () => {
      calls++;
      throw new Error('unknown');
    },
  };
  const session = new AgentControlSession(port, 'agent');
  session.activate(first);
  await session.refresh();
  const ticket = session.ticket()!;
  await Promise.all([session.change(ticket), session.change(ticket)]);
  assert.equal(calls, 1);
  assert.equal(session.getSnapshot().pending, 'Pause');
  session.suspend();
  session.activate(first);
  await session.refresh();
  assert.equal(session.getSnapshot().pending, 'Pause');
  assert.equal(session.ticket(), null);
  status = 'Paused';
  await session.refresh();
  assert.equal(session.getSnapshot().pending, null);
  assert.equal(calls, 1);
});
