import assert from 'node:assert/strict';
import { test } from 'node:test';
import { agentControlRevision, agentControlRevisionMax } from '../agents/agent-control-revision.ts';
import { AgentControlSession } from '../agents/agent-control-session.ts';

test('lifecycle revisions remain exact, canonical and inside the relay signed integer range', () => {
  for (const value of [
    '0',
    '01',
    '-1',
    '+1',
    '1.0',
    '1e2',
    ' 1',
    '9223372036854775808',
    '9'.repeat(10000),
    1,
    null,
  ])
    assert.equal(agentControlRevision(value), null);
  assert.equal(agentControlRevision('9007199254740993'), 9007199254740993n);
  assert.equal(agentControlRevision('9223372036854775807'), agentControlRevisionMax);
});

test('the final relay revision is observable but never offers an impossible lifecycle mutation', async () => {
  const scope = { identity: 'owner', device: 'device', relay: 'relay', epoch: 1 };
  const session = new AgentControlSession(
    {
      scope: () => scope,
      read: async () => ({
        status: 'Active',
        revision: String(agentControlRevisionMax),
        pending: null,
        operation_id: null,
        outcome: 'Observed',
      }),
      control: async () => {
        throw Error('must not dispatch');
      },
      retry: async () => {
        throw Error('must not retry');
      },
    },
    'agent',
  );
  session.activate(scope);
  await session.refresh();
  assert.equal(session.getSnapshot().phase, 'ready');
  assert.equal(session.getSnapshot().status, 'Active');
  assert.equal(session.ticket(), null);
  assert.equal(session.retryTicket(), null);
});
