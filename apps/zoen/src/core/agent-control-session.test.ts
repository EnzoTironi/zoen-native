import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentControlSession, type AgentControlPort } from '../agents/agent-control-session.ts';
import { accountScopeKey, type AccountScope } from './account-scope.ts';
import type { AgentControlDto } from './generated/AgentControlDto.ts';
const first: AccountScope = { identity: 'a', device: 'd', relay: 'r', epoch: 1 };
function fixture() {
  let scope = first;
  let record: AgentControlDto = {
    status: 'Active',
    revision: '1',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  };
  const calls: string[] = [];
  const port: AgentControlPort = {
    scope: () => scope,
    retry: async (_agent, action, _scope, revision, operationId) => {
      calls.push(`retry:${action}:${revision}:${operationId}`);
      record = {
        status: action === 'Pause' ? 'Paused' : action === 'Resume' ? 'Active' : 'Revoked',
        revision: String(BigInt(revision) + 1n),
        pending: null,
        operation_id: operationId,
        outcome: 'Confirmed',
      };
    },
    read: async () => ({ ...record }),
    control: async (_agent, action, _scope, revision) => {
      calls.push(`${action}:${revision}`);
      record = { ...record, pending: action, operation_id: 'd'.repeat(64), outcome: 'Unknown' };
      throw Error('lost ACK');
    },
  };
  return {
    port,
    calls,
    set: (value: AgentControlDto) => {
      record = value;
    },
    epoch: () => {
      scope = { ...scope, epoch: scope.epoch + 1 };
      return scope;
    },
  };
}
test('cold session restores durable unknown and cannot dispatch contradictory retry', async () => {
  const f = fixture();
  const session = new AgentControlSession(f.port, 'agent');
  session.activate(first);
  await session.refresh();
  const ticket = session.ticket()!;
  await Promise.all([session.change(ticket), session.change(ticket)]);
  assert.deepEqual(f.calls, ['Pause:1']);
  session.retire();
  const cold = new AgentControlSession(f.port, 'agent');
  cold.activate(first);
  await cold.refresh();
  assert.equal(cold.getSnapshot().pending, 'Pause');
  assert.equal(cold.ticket(), null);
  // Same status at the old revision does not resolve the operation.
  await cold.refresh();
  assert.equal(cold.ticket(), null);
  f.set({
    status: 'Paused',
    revision: '2',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  });
  await cold.refresh();
  assert.equal(cold.ticket()?.action, 'Resume');
  assert.deepEqual(f.calls, ['Pause:1']);
});
test('new canonical revision invalidates the old handler even with identical status', async () => {
  const f = fixture();
  const session = new AgentControlSession(f.port, 'agent');
  session.activate(first);
  await session.refresh();
  const old = session.ticket()!;
  f.set({
    status: 'Active',
    revision: '3',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  });
  await session.refresh();
  await session.change(old);
  assert.deepEqual(f.calls, []);
  assert.equal(session.ticket()?.revision, '3');
});
test('account reopen invalidates old handlers and requires fresh durable read', async () => {
  const f = fixture();
  const session = new AgentControlSession(f.port, 'agent');
  session.activate(first);
  await session.refresh();
  const old = session.ticket()!;
  const next = f.epoch();
  session.activate(next);
  await session.change(old);
  assert.equal(session.getSnapshot().scopeKey, accountScopeKey(next));
  assert.equal(session.ticket(), null);
  await session.refresh();
  assert.deepEqual(f.calls, []);
});
test('canonical final revocation has no control ticket', async () => {
  const f = fixture();
  f.set({
    status: 'Revoked',
    revision: '2',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  });
  const session = new AgentControlSession(f.port, 'agent');
  session.activate(first);
  await session.refresh();
  assert.equal(session.ticket(), null);
});

test('a ticket cannot cross into a different agent session', async () => {
  const f = fixture();
  const a = new AgentControlSession(f.port, 'agent-a');
  const b = new AgentControlSession(f.port, 'agent-b');
  a.activate(first);
  b.activate(first);
  await a.refresh();
  await b.refresh();
  await b.change(a.ticket()!);
  assert.deepEqual(f.calls, []);
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
test('a late read cannot cross an account epoch', async () => {
  const f = fixture();
  const held = deferred<AgentControlDto>();
  const session = new AgentControlSession({ ...f.port, read: () => held.promise }, 'agent');
  session.activate(first);
  const refresh = session.refresh();
  const next = f.epoch();
  session.activate(next);
  held.resolve({
    status: 'Active',
    revision: '1',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  });
  await refresh;
  assert.equal(session.getSnapshot().scopeKey, accountScopeKey(next));
  assert.equal(session.getSnapshot().status, null);
  assert.equal(session.ticket(), null);
});
test('late dispatch completion after suspension cannot replay a consumed ticket', async () => {
  const f = fixture();
  const held = deferred<void>();
  const session = new AgentControlSession(
    {
      ...f.port,
      control: async (...args) => {
        try {
          await f.port.control(...args);
        } catch {
          /* Persisted unknown in the port fixture. */
        }
        await held.promise;
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  const old = session.ticket()!;
  const change = session.change(old);
  session.suspend();
  session.activate(first);
  await session.refresh();
  held.resolve();
  await change;
  await session.change(old);
  assert.equal(session.ticket(), null);
  assert.equal(session.getSnapshot().pending, 'Pause');
  assert.deepEqual(f.calls, ['Pause:1']);
});
test('failed reconciliation retains unknown until a canonical fence is observed', async () => {
  const f = fixture();
  let offline = false;
  const session = new AgentControlSession(
    {
      ...f.port,
      read: async (...args) => {
        if (offline) throw Error('offline');
        return f.port.read(...args);
      },
    },
    'agent',
  );
  session.activate(first);
  await session.refresh();
  offline = true;
  await session.change(session.ticket()!);
  assert.equal(session.getSnapshot().phase, 'failed');
  assert.equal(session.getSnapshot().pending, 'Pause');
  offline = false;
  await session.refresh();
  assert.equal(session.ticket(), null);
  f.set({
    status: 'Revoked',
    revision: '2',
    pending: null,
    operation_id: null,
    outcome: 'Superseded',
  });
  await session.refresh();
  assert.equal(session.getSnapshot().pending, null);
  assert.equal(session.ticket(), null);
});

test('only an explicit current retry ticket resends the original durable intent once', async () => {
  const f = fixture();
  const session = new AgentControlSession(f.port, 'agent');
  session.activate(first);
  await session.refresh();
  await session.change(session.ticket()!);
  const cold = new AgentControlSession(f.port, 'agent');
  cold.activate(first);
  await cold.refresh();
  const stale = cold.retryTicket()!;
  const next = f.epoch();
  cold.activate(next);
  await cold.retry(stale);
  await cold.refresh();
  assert.deepEqual(f.calls, ['Pause:1']);
  const current = cold.retryTicket()!;
  await Promise.all([cold.retry(current), cold.retry(current)]);
  await cold.retry(current);
  assert.deepEqual(f.calls, ['Pause:1', `retry:Pause:1:${'d'.repeat(64)}`]);
  assert.equal(cold.getSnapshot().pending, null);
  assert.equal(cold.getSnapshot().status, 'Paused');
});
