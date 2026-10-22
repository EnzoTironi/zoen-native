import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createEnrollmentPort } from '../agent-enrollment/core-port.ts';
import type { EnrollmentReview, EnrollmentScope } from '../agent-enrollment/session.ts';
import type { AgentEnrollmentPreviewDto } from './generated/AgentEnrollmentPreviewDto.ts';
import type { AgentEnrollmentStatusDto } from './generated/AgentEnrollmentStatusDto.ts';
import type { Persona } from './generated/Persona.ts';
import type { ClientState } from './store.ts';
import type { Command, CommandResult } from './types.ts';

const scope: EnrollmentScope = { owner: 'owner', device: 'phone', relay: 'https://relay.example' };
const preview: AgentEnrollmentPreviewDto = {
  agent_id: 'agent',
  offer_digest: 'digest',
  name: 'Booking helper',
  handle: 'booking',
  persona: 'Helps plan trips.',
  rules: ['Ask before booking.'],
  pairing_check: '123456',
  expires_ms: '9007199254740993',
};
const review: EnrollmentReview = {
  agent: preview.agent_id,
  digest: preview.offer_digest,
  name: preview.name,
  handle: preview.handle,
  persona: preview.persona,
  rules: preview.rules,
  pairing: preview.pairing_check,
  expires: 9007199254740993n,
};
const receipt: AgentEnrollmentStatusDto = {
  preview,
  delivery: {
    receipt_id: 'receipt',
    agent_id: preview.agent_id,
    offer_digest: preview.offer_digest,
    outcome: 'Unknown',
  },
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function fixture() {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: scope.owner,
        device_id: scope.device,
        relay_url: scope.relay,
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  } as ClientState;
  const calls: { kind: 'read' | 'refresh'; command: Command }[] = [];
  let handler: (command: Command) => Promise<unknown> = async () => null;
  const run = async <C extends Command>(
    kind: 'read' | 'refresh',
    command: C,
  ): Promise<CommandResult<C>> => {
    calls.push({ kind, command });
    return (await handler(command)) as CommandResult<C>;
  };
  const port = createEnrollmentPort({
    driver: { request: <C extends Command>(command: C) => run('read', command) } as any,
    request: <C extends Command>(command: C) => run('refresh', command),
    getSnapshot: () => state,
  });
  return {
    port,
    state,
    calls,
    handle: (next: typeof handler) => {
      handler = next;
    },
  };
}

test('preview uses the generated domain command and projects lossless expiry without exposing secrets', async () => {
  const f = fixture();
  f.handle(async () => preview);
  assert.deepEqual(await f.port.preview('transient-code', scope), review);
  assert.deepEqual(f.calls, [
    {
      kind: 'read',
      command: {
        op: 'agent_enrollment_preview',
        code: 'transient-code',
        expected_identity_id: 'owner',
        expected_device_id: 'phone',
      },
    },
  ]);
});

test('approval submits only the observed digest and comparison string with current owner/device', async () => {
  const f = fixture();
  f.handle(async () => receipt.delivery);
  assert.deepEqual(await f.port.approve('transient-code', review, scope), {
    receipt: 'receipt',
    agent: 'agent',
    digest: 'digest',
    outcome: 'Unknown',
  });
  assert.deepEqual(f.calls, [
    {
      kind: 'refresh',
      command: {
        op: 'agent_enrollment_approve',
        code: 'transient-code',
        observed_offer_digest: 'digest',
        confirmed_pairing_check: '123456',
        expected_identity_id: 'owner',
        expected_device_id: 'phone',
      },
    },
  ]);
});

test('read-only status distinguishes supported-empty from an unsupported/failed probe', async () => {
  const f = fixture();
  assert.equal(await f.port.receipt(scope), null);
  f.handle(async () => {
    throw Error('unsupported command');
  });
  await assert.rejects(f.port.receipt(scope));
  assert.deepEqual(
    f.calls.map((call) => call.command.op),
    ['agent_enrollment_status', 'agent_enrollment_status'],
  );
  assert.ok(f.calls.every((call) => call.kind === 'read'));
});

test('public receipt restores only matching preview and delivery, with the CAS token', async () => {
  const f = fixture();
  f.handle(async () => receipt);
  assert.deepEqual(await f.port.receipt(scope), { review, receipt: 'receipt', outcome: 'Unknown' });
  for (const changed of [{ agent_id: 'other' }, { offer_digest: 'other' }, { receipt_id: '' }]) {
    f.handle(async () => ({ ...receipt, delivery: { ...receipt.delivery, ...changed } }));
    await assert.rejects(f.port.receipt(scope));
  }
});

for (const change of ['owner', 'device', 'relay', 'lock', 'phase', 'erase', 'demo'] as const) {
  test(`changed ${change} rejects before dispatch, including signing and dismiss`, async () => {
    const f = fixture();
    if (change === 'owner') f.state.snapshot!.account!.identity_id = 'other';
    if (change === 'device') f.state.snapshot!.account!.device_id = 'other';
    if (change === 'relay') f.state.snapshot!.account!.relay_url = 'https://other.example';
    if (change === 'lock') f.state.snapshot!.account!.unlocked = false;
    if (change === 'phase') f.state.phase = 'opening';
    if (change === 'erase') f.state.erasePending = true;
    if (change === 'demo') f.state.mode = 'demo';
    await assert.rejects(f.port.preview('code', scope));
    await assert.rejects(f.port.approve('code', review, scope));
    await assert.rejects(f.port.receipt(scope));
    await assert.rejects(f.port.dismiss('receipt', review, scope));
    await assert.rejects(f.port.registered(review, scope));
    assert.equal(f.calls.length, 0);
  });
}

test('a late native result cannot be adopted after a device change', async () => {
  const f = fixture();
  const pending = deferred<AgentEnrollmentPreviewDto>();
  f.handle(() => pending.promise);
  const task = f.port.preview('code', scope);
  f.state.snapshot!.account!.device_id = 'other';
  pending.resolve(preview);
  await assert.rejects(task);
});

test('registered navigation requires exact public Agent ID, handle, owner and owner-only status', async () => {
  const f = fixture();
  const person = {
    id: 'agent',
    handle: 'booking',
    kind: 'Agent',
    owner_id: 'owner',
    is_mine: true,
  } as Persona;
  for (const changed of [
    { id: 'other' },
    { handle: 'booking-other' },
    { kind: 'Person' },
    { owner_id: 'other' },
    { is_mine: false },
  ]) {
    f.calls.length = 0;
    f.handle(async (command) => {
      assert.equal(command.op, 'find_people');
      return [{ ...person, ...changed }];
    });
    assert.equal(await f.port.registered(review, scope), false);
    assert.equal(f.calls.length, 1);
  }
  for (const status of ['Active', 'Paused', 'Revoked']) {
    f.calls.length = 0;
    f.handle(async (command) => (command.op === 'find_people' ? [person] : status));
    assert.equal(await f.port.registered(review, scope), status !== 'Revoked');
    assert.deepEqual(f.calls, [
      { kind: 'refresh', command: { op: 'find_people', query: 'booking' } },
      { kind: 'read', command: { op: 'agent_status', agent_id: 'agent' } },
    ]);
  }
});

test('explicit dismissal uses the exact receipt/digest CAS without granting, revoking or sending', async () => {
  const f = fixture();
  f.handle(async () => true);
  assert.equal(await f.port.dismiss('receipt', review, scope), true);
  assert.deepEqual(f.calls, [
    {
      kind: 'refresh',
      command: {
        op: 'agent_enrollment_dismiss',
        receipt_id: 'receipt',
        observed_offer_digest: 'digest',
        expected_identity_id: 'owner',
        expected_device_id: 'phone',
      },
    },
  ]);
});
