import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createDefinitionPort } from '../agent-definition/core-port.ts';
import type { DefinitionPort, DefinitionScope } from '../agent-definition/session.ts';
import type { ClientStore, ClientState } from './store.ts';
import type { Command } from './types.ts';
import type { AgentDefinitionDto } from './generated/AgentDefinitionDto.ts';

const scope: DefinitionScope = {
  agent: 'agent',
  owner: 'owner',
  device: 'phone',
  relay: 'https://relay.example',
};
const current: AgentDefinitionDto = {
  agent_id: 'agent',
  owner_id: 'owner',
  device_id: 'phone',
  relay_url: scope.relay,
  status: 'Active',
  version: 4,
  definition_sha256: 'base',
  persona: 'Before',
  rules: ['a', 'b'],
};
const proposed = {
  ...current,
  version: 5,
  definition_sha256: 'next',
  persona: '  🪷\nAfter  ',
  rules: [' b ', 'b', 'a'],
};
const review = { current, proposed, preview_digest: 'digest' };
const receipt = { receipt_id: 'receipt', review, outcome: 'Unknown' as const, current };
function fixture() {
  const account = {
    identity_id: 'owner',
    device_id: 'phone',
    relay_url: scope.relay,
    registered: true,
    unlocked: true,
    setup_pending: false,
  };
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: { account },
  } as unknown as ClientState;
  const calls: { command: Command; path: string }[] = [];
  let answer: (command: Command) => Promise<unknown> = async (command) =>
    command.op === 'agent_definition_read'
      ? current
      : command.op === 'agent_definition_preview'
        ? review
        : command.op === 'agent_definition_dismiss'
          ? true
          : receipt;
  const request = async (command: Command, path: string) => {
    calls.push({ command, path });
    return answer(command);
  };
  const store = {
    getSnapshot: () => state,
    driver: { request: (command: Command) => request(command, 'driver') },
    request: (command: Command) => request(command, 'store'),
  } as unknown as Pick<ClientStore, 'getSnapshot' | 'driver' | 'request'>;
  return {
    port: createDefinitionPort(store),
    state,
    account,
    calls,
    handle: (fn: typeof answer) => {
      answer = fn;
    },
  };
}
const invoke: Record<keyof DefinitionPort, (port: DefinitionPort) => Promise<unknown>> = {
  read: (port) => port.read(scope),
  preview: (port) => port.preview(scope, current, proposed.persona, proposed.rules),
  update: (port) => port.update(scope, review),
  reconcile: (port) => port.reconcile(scope),
  dismiss: (port) => port.dismiss(scope, receipt),
};
test('actual generated command adapter uses exact scope/base/digest and human text without normalization', async () => {
  const f = fixture();
  for (const action of Object.values(invoke)) await action(f.port);
  for (const { command } of f.calls) {
    assert.ok(
      'agent_id' in command && 'expected_identity_id' in command && 'expected_device_id' in command,
    );
    assert.equal(command.agent_id, 'agent');
    assert.equal(command.expected_identity_id, 'owner');
    assert.equal(command.expected_device_id, 'phone');
  }
  assert.deepEqual(
    f.calls.map((call) => call.path),
    ['driver', 'driver', 'store', 'driver', 'store'],
  );
  const preview = f.calls[1]!.command,
    update = f.calls[2]!.command,
    dismiss = f.calls[4]!.command;
  assert.equal(preview.op, 'agent_definition_preview');
  assert.equal(update.op, 'agent_definition_update');
  assert.equal(dismiss.op, 'agent_definition_dismiss');
  if (
    preview.op !== 'agent_definition_preview' ||
    update.op !== 'agent_definition_update' ||
    dismiss.op !== 'agent_definition_dismiss'
  )
    throw Error('Wrong command');
  assert.equal(preview.persona, proposed.persona);
  assert.deepEqual(preview.rules, proposed.rules);
  assert.equal(update.observed_version, current.version);
  assert.equal(update.observed_definition_sha256, current.definition_sha256);
  assert.equal(update.observed_preview_digest, review.preview_digest);
  assert.equal(update.persona, proposed.persona);
  assert.deepEqual(update.rules, proposed.rules);
  assert.equal(dismiss.receipt_id, 'receipt');
  assert.equal(dismiss.observed_preview_digest, 'digest');
});
for (const method of Object.keys(invoke) as (keyof DefinitionPort)[]) {
  for (const field of ['identity_id', 'device_id', 'relay_url'] as const) {
    test(`${method} fences ${field} before dispatch and after await`, async () => {
      const f = fixture();
      f.account[field] = 'changed';
      await assert.rejects(invoke[method](f.port));
      assert.equal(f.calls.length, 0);
      f.account[field] =
        field === 'identity_id' ? 'owner' : field === 'device_id' ? 'phone' : scope.relay;
      let resolve!: (value: unknown) => void;
      f.handle(
        () =>
          new Promise((yes) => {
            resolve = yes;
          }),
      );
      const pending = invoke[method](f.port);
      f.account[field] = 'changed';
      resolve(
        method === 'read'
          ? current
          : method === 'preview'
            ? review
            : method === 'dismiss'
              ? true
              : receipt,
      );
      await assert.rejects(pending);
      assert.equal(f.calls.length, 1);
    });
  }
}
test('locked/setup/account mode changes and session erasure cannot read or edit', async () => {
  for (const change of [
    (f: ReturnType<typeof fixture>) => {
      f.account.unlocked = false;
    },
    (f: ReturnType<typeof fixture>) => {
      f.account.registered = false;
    },
    (f: ReturnType<typeof fixture>) => {
      f.account.setup_pending = true;
    },
    (f: ReturnType<typeof fixture>) => {
      f.state.erasePending = true;
    },
    (f: ReturnType<typeof fixture>) => {
      f.state.mode = 'demo';
    },
  ]) {
    const f = fixture();
    change(f);
    await assert.rejects(f.port.read(scope));
    assert.equal(f.calls.length, 0);
  }
});
