import assert from 'node:assert/strict';
import test from 'node:test';
import { createAgentControlPort } from '../agents/agent-control-port.ts';
import type { ClientStore } from './store.ts';

function fixture() {
  let epoch = 1;
  const calls: string[] = [];
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
        return 'Active';
      },
    },
    request: async (command: { op: string }) => {
      calls.push(command.op);
    },
  } as unknown as ClientStore;
  const port = createAgentControlPort(store);
  return {
    port,
    calls,
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
  assert.equal(await port.read('agent', scope), 'Active');
  await port.control('agent', 'Pause', scope);
  assert.deepEqual(calls, ['agent_profile', 'agent_status', 'agent_profile', 'control_agent']);
});

test('changed persona owner prevents reads and control dispatch', async () => {
  const { port, calls, changeOwner } = fixture();
  const scope = port.scope()!;
  changeOwner();
  await assert.rejects(port.read('agent', scope), /not owned/);
  await assert.rejects(port.control('agent', 'Pause', scope), /not owned/);
  assert.deepEqual(calls, ['agent_profile', 'agent_profile']);
});

test('session change during ownership lookup prevents control dispatch', async () => {
  const { port, calls, switchDuringProfile } = fixture();
  const scope = port.scope()!;
  switchDuringProfile();
  await assert.rejects(port.control('agent', 'Pause', scope), /session has changed/);
  assert.deepEqual(calls, ['agent_profile']);
});
