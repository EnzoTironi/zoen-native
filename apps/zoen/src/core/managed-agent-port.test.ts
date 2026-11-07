import assert from 'node:assert/strict';
import { test } from 'node:test';
import { managedAgentPort } from '../managed-agent/core-port.ts';
import { accountScope } from './account-scope.ts';
import { ClientStore } from './store.ts';
import { Driver, empty, erasureAccount, deferred } from './store.fixtures.ts';
import type { ManagedEnrollmentDto } from './generated/ManagedEnrollmentDto.ts';
import type { Command } from './types.ts';
const installed: ManagedEnrollmentDto = {
  operation_id: 'operation',
  agent_id: 'agent',
  state: 'registered',
  name: 'Assistant',
  handle: 'agent_handle',
  preview: null,
};
async function fixture() {
  const driver = new Driver();
  const snapshot = { ...empty, account: { ...erasureAccount } };
  driver.opening = async () => snapshot;
  const store = new ClientStore(driver);
  await store.open('account');
  const owner = accountScope(store);
  if (!owner) throw Error('Owner unavailable');
  const calls: Command[] = [];
  let handle: (command: Command) => Promise<unknown> = async () => null;
  driver.operation = async (command) => {
    calls.push(command);
    return command.op === 'snapshot' ? snapshot : handle(command);
  };
  return {
    store,
    owner,
    calls,
    port: managedAgentPort(store),
    snapshot,
    handle: (value: typeof handle) => {
      handle = value;
    },
  };
}
test('creation and status bind owner/device and never dispatch grants, budget or pairing codes', async () => {
  const f = await fixture();
  try {
    f.handle(async () => installed);
    await f.port.start('Assistant', f.owner);
    await f.port.status(f.owner);
    assert.deepEqual(f.calls, [
      {
        op: 'managed_agent_start',
        name: 'Assistant',
        expected_identity_id: f.owner.identity,
        expected_device_id: f.owner.device,
      },
      {
        op: 'managed_agent_status',
        expected_identity_id: f.owner.identity,
        expected_device_id: f.owner.device,
      },
    ]);
  } finally {
    await f.store.close();
  }
});
test('canonical discovery must confirm exact agent, owner, handle and active status', async () => {
  const f = await fixture();
  const candidate = {
    id: 'agent',
    handle: 'agent_handle',
    kind: 'Agent',
    is_mine: true,
    owner_id: f.owner.identity,
  };
  try {
    for (const changed of [
      { id: 'wrong' },
      { owner_id: 'wrong' },
      { handle: 'wrong' },
      { is_mine: false },
      { kind: 'Person' },
    ]) {
      f.handle(async (command) =>
        command.op === 'find_people' ? [{ ...candidate, ...changed }] : 'Active',
      );
      await assert.rejects(f.port.verify(installed, f.owner));
    }
    f.handle(async (command) => (command.op === 'find_people' ? [candidate] : 'Revoked'));
    await assert.rejects(f.port.verify(installed, f.owner));
    f.handle(async (command) => (command.op === 'find_people' ? [candidate] : 'Active'));
    await f.port.verify(installed, f.owner);
  } finally {
    await f.store.close();
  }
});
test('same-account native reopen rejects a late status reply and old completion', async () => {
  const f = await fixture();
  const gate = deferred<unknown>();
  try {
    f.handle(() => gate.promise);
    const result = f.port.status(f.owner);
    await f.store.close();
    await f.store.open('account');
    gate.resolve(installed);
    await assert.rejects(result);
    await assert.rejects(f.port.complete(installed, f.owner));
    assert.equal(f.calls.filter((command) => command.op === 'managed_agent_complete').length, 0);
  } finally {
    await f.store.close();
  }
});
test('chat and completion pass the exact operation and agent with scoped native commands', async () => {
  const f = await fixture();
  try {
    f.handle(async (command) => (command.op === 'managed_agent_chat' ? 'space' : true));
    assert.equal(await f.port.chat(installed, f.owner), 'space');
    await f.port.complete(installed, f.owner);
    assert.deepEqual(
      f.calls.filter((command) => command.op.startsWith('managed_agent_')),
      [
        {
          op: 'managed_agent_chat',
          operation_id: 'operation',
          agent_id: 'agent',
          expected_identity_id: f.owner.identity,
          expected_device_id: f.owner.device,
        },
        {
          op: 'managed_agent_complete',
          operation_id: 'operation',
          agent_id: 'agent',
          expected_identity_id: f.owner.identity,
          expected_device_id: f.owner.device,
        },
      ],
    );
    assert.ok(
      !f.calls.some((command) =>
        ['set_trust', 'raise_budget', 'start_direct'].includes(command.op),
      ),
    );
  } finally {
    await f.store.close();
  }
});

test('recovery binds both sides of the native call to the current account without exposing ciphertext', async () => {
  const f = await fixture();
  try {
    f.handle(async () => ({ requests: [], next_operation: null }));
    await f.port.discover('cursor', f.owner);
    f.handle(async () => installed);
    await f.port.recover('operation', f.owner);
    assert.deepEqual(f.calls, [
      {
        op: 'managed_agent_discover',
        after_operation: 'cursor',
        expected_identity_id: f.owner.identity,
        expected_device_id: f.owner.device,
      },
      {
        op: 'managed_agent_recover',
        operation_id: 'operation',
        expected_identity_id: f.owner.identity,
        expected_device_id: f.owner.device,
      },
    ]);
  } finally {
    await f.store.close();
  }
});
