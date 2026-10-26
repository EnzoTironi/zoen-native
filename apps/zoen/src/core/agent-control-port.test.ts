import assert from 'node:assert/strict';
import test from 'node:test';
import { createAgentControlPort } from '../agents/agent-control-port.ts';
import type { ClientStore } from './store.ts';

function fixture() {
  let epoch = 1;
  const calls: string[] = [];
  const sent: unknown[] = [];
  let owner = 'owner';
  let beforeProfile: (() => void) | undefined;
  const store = {
    get sessionEpoch() {
      return epoch;
    },
    isSessionRetiring: false,
    getSnapshot: () => ({
      phase: 'ready',
      mode: 'account',
      snapshot: {
        account: {
          registered: true,
          unlocked: true,
          setup_pending: false,
          identity_id: 'owner',
          device_id: 'device',
          relay_url: 'relay',
        },
      },
    }),
    driver: {
      request: async (command: { op: string }) => {
        calls.push(command.op);
        if (command.op === 'agent_profile') {
          beforeProfile?.();
          return { persona: { id: 'agent', kind: 'Agent', is_mine: true, owner_id: owner } };
        }
        return {
          status: 'Active',
          revision: '1',
          pending: null,
          operation_id: null,
          outcome: 'Observed',
        };
      },
    },
    request: async (command: { op: string }) => {
      calls.push(command.op);
      sent.push(command);
    },
  } as unknown as ClientStore;
  const port = createAgentControlPort(store);
  return {
    port,
    calls,
    sent,
    changeOwner: () => {
      owner = 'other';
    },
    switchDuringProfile: () => {
      beforeProfile = () => {
        epoch++;
      };
    },
  };
}

test('canonical status and lifecycle writes require fresh ownership proof', async () => {
  const { port, calls } = fixture();
  const scope = port.scope()!;
  assert.deepEqual(await port.read('agent', scope), {
    status: 'Active',
    revision: '1',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  });
  await port.control('agent', 'Pause', scope, '1');
  assert.deepEqual(calls, [
    'agent_profile',
    'agent_control_read',
    'agent_profile',
    'agent_control_change',
  ]);
});

test('changed persona owner prevents reads and control dispatch', async () => {
  const { port, calls, changeOwner } = fixture();
  const scope = port.scope()!;
  changeOwner();
  await assert.rejects(port.read('agent', scope), /not owned/);
  await assert.rejects(port.control('agent', 'Pause', scope, '1'), /not owned/);
  assert.deepEqual(calls, ['agent_profile', 'agent_profile']);
});

test('session change during ownership lookup prevents control dispatch', async () => {
  const { port, calls, switchDuringProfile } = fixture();
  const scope = port.scope()!;
  switchDuringProfile();
  await assert.rejects(port.control('agent', 'Pause', scope, '1'), /session has changed/);
  assert.deepEqual(calls, ['agent_profile']);
});

test('explicit retry carries the reviewed durable identity and same owner device', async () => {
  const { port, calls, sent } = fixture();
  const scope = port.scope()!;
  const operation = 'd'.repeat(64);
  await port.retry('agent', 'Pause', scope, '7', operation);
  assert.deepEqual(calls, ['agent_profile', 'agent_control_retry']);
  assert.deepEqual(sent, [
    {
      op: 'agent_control_retry',
      agent_id: 'agent',
      action: 'Pause',
      observed_revision: '7',
      observed_operation_id: operation,
      expected_identity_id: 'owner',
      expected_device_id: 'device',
    },
  ]);
});
test('a changed owner or account epoch prevents explicit retry dispatch', async () => {
  const foreign = fixture();
  const scope = foreign.port.scope()!;
  foreign.changeOwner();
  await assert.rejects(
    foreign.port.retry('agent', 'Pause', scope, '1', 'd'.repeat(64)),
    /not owned/,
  );
  assert.deepEqual(foreign.sent, []);
  const retired = fixture();
  const previous = retired.port.scope()!;
  retired.switchDuringProfile();
  await assert.rejects(
    retired.port.retry('agent', 'Pause', previous, '1', 'd'.repeat(64)),
    /session has changed/,
  );
  assert.deepEqual(retired.sent, []);
});
