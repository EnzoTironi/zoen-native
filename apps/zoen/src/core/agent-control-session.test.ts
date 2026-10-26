import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentControlSession, type AgentControlPort } from '../agents/agent-control-session.ts';
import { accountScopeKey, type AccountScope } from './account-scope.ts';

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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('an in-flight dispatch stays pending across route suspension and late completion', async () => {
  const sent = deferred<void>();
  let calls = 0;
  const session = new AgentControlSession(
    {
      scope: () => first,
      read: async () => 'Active',
      control: () => {
        calls++;
        return sent.promise;
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  const ticket = session.ticket()!;
  const change = session.change(ticket);
  session.suspend();
  session.activate(first);
  await session.refresh();
  assert.equal(session.getSnapshot().pending, 'Pause');
  sent.resolve();
  await change;
  await session.change(ticket);
  assert.equal(calls, 1);
  assert.equal(session.ticket(), null);
});

test('owner ABA and retirement invalidate old handlers and late dispatch completion', async () => {
  let scope = first;
  const sent = deferred<void>();
  let calls = 0;
  const session = new AgentControlSession(
    {
      scope: () => scope,
      read: async () => 'Active',
      control: () => {
        calls++;
        return sent.promise;
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  const ticket = session.ticket()!;
  const change = session.change(ticket);
  session.retire();
  scope = { ...first, identity: 'b', epoch: 2 };
  session.activate(scope);
  scope = { ...first, epoch: 3 };
  session.activate(scope);
  await session.refresh();
  sent.resolve();
  await change;
  await session.change(ticket);
  assert.equal(calls, 1);
  assert.equal(session.getSnapshot().pending, null);
  assert.equal(session.getSnapshot().busy, false);
  assert.equal(session.ticket()?.generation === ticket.generation, false);
});

test('read failure preserves uncertainty and canonical revocation ends control', async () => {
  let result: 'Active' | 'Revoked' | 'error' = 'Active';
  let calls = 0;
  const session = new AgentControlSession(
    {
      scope: () => first,
      read: async () => {
        if (result === 'error') throw Error('offline');
        return result;
      },
      control: async () => {
        calls++;
        throw Error('ACK lost');
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  result = 'error';
  await session.change(session.ticket()!);
  assert.equal(session.getSnapshot().phase, 'failed');
  assert.equal(session.getSnapshot().pending, 'Pause');
  assert.equal(session.getSnapshot().busy, false);
  result = 'Revoked';
  await session.refresh();
  assert.equal(session.getSnapshot().status, 'Revoked');
  assert.equal(session.getSnapshot().pending, null);
  assert.equal(session.ticket(), null);
  assert.equal(calls, 1);
});

test('a newer canonical observation invalidates a captured action', async () => {
  let calls = 0;
  const session = new AgentControlSession(
    {
      scope: () => first,
      read: async () => 'Active',
      control: async () => {
        calls++;
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  const ticket = session.ticket()!;
  await session.refresh();
  await session.change(ticket);
  assert.equal(calls, 0);
});

test('a lost Resume ACK reconciles only through the matching canonical status', async () => {
  let status: 'Active' | 'Paused' = 'Paused';
  let calls = 0;
  const session = new AgentControlSession(
    {
      scope: () => first,
      read: async () => status,
      control: async (_agent, action) => {
        assert.equal(action, 'Resume');
        calls++;
        throw Error('ACK lost');
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  await session.change(session.ticket()!);
  assert.equal(session.getSnapshot().pending, 'Resume');
  await session.refresh();
  assert.equal(session.getSnapshot().pending, 'Resume');
  status = 'Active';
  await session.refresh();
  assert.equal(session.getSnapshot().pending, null);
  assert.equal(session.ticket()?.action, 'Pause');
  assert.equal(calls, 1);
});
