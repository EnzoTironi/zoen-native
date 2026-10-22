import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createDefinitionPort } from '../agent-definition/core-port.ts';
import {
  DefinitionSession,
  definitionScopeKey,
  type DefinitionScope,
} from '../agent-definition/session.ts';
import type { ClientState, ClientStore } from './store.ts';
import type { Command } from './types.ts';
import type { AgentDefinitionDto } from './generated/AgentDefinitionDto.ts';
import type { AgentDefinitionUpdateDto } from './generated/AgentDefinitionUpdateDto.ts';

function fixture(rawRelay: string, dtoRelay = rawRelay) {
  const scope: DefinitionScope = {
    owner: 'owner',
    device: 'phone',
    agent: 'agent',
    relay: rawRelay,
  };
  const account = {
    identity_id: scope.owner,
    device_id: scope.device,
    relay_url: rawRelay,
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
  const baseline: AgentDefinitionDto = {
    agent_id: scope.agent,
    owner_id: scope.owner,
    device_id: scope.device,
    relay_url: dtoRelay,
    status: 'Active',
    version: 1,
    definition_sha256: 'base',
    persona: 'Before',
    rules: ['Ask first.'],
  };
  const review = {
    current: baseline,
    proposed: {
      ...baseline,
      version: 2,
      definition_sha256: 'next',
      persona: '  After 🪷  ',
      rules: [' Ask first. ', 'Preserve exact text.'],
    },
    preview_digest: 'digest',
  };
  const receipt: AgentDefinitionUpdateDto = {
    receipt_id: 'receipt',
    review,
    outcome: 'Confirmed',
    current: review.proposed,
  };
  let held: AgentDefinitionUpdateDto | null = null;
  const calls: Command[] = [];
  let answer: (command: Command) => Promise<unknown> = async (command) => {
    if (command.op === 'agent_definition_reconcile') return held;
    if (command.op === 'agent_definition_read') return baseline;
    if (command.op === 'agent_definition_preview') return review;
    if (command.op === 'agent_definition_update') {
      held = receipt;
      return receipt;
    }
    return true;
  };
  const request = async (command: Command) => {
    calls.push(command);
    return answer(command);
  };
  const store = {
    getSnapshot: () => state,
    driver: { request },
    request,
  } as unknown as Pick<ClientStore, 'getSnapshot' | 'driver' | 'request'>;
  const port = createDefinitionPort(store),
    session = new DefinitionSession(port);
  session.activate(scope);
  return {
    scope,
    account,
    baseline,
    review,
    receipt,
    port,
    session,
    calls,
    handle: (value: typeof answer) => {
      answer = value;
    },
  };
}

// Core returns the exact configured account relay; UI does not infer URL equivalence.
for (const raw of [
  'https://relay.example/',
  'https://relay.example////',
  'wss://relay.example',
  'wss://relay.example/',
  'ws://127.0.0.1:8891/',
  'http://127.0.0.1:8891///',
  'relay.example',
  'relay.example/',
  'https://Relay.Example/',
  'https://relay.example:443/',
  'wss://Relay.Example:443/',
]) {
  test(`exact raw DTO relay ${raw} permits read/review/update and held reconciliation`, async () => {
    const f = fixture(raw);
    await f.session.load();
    assert.equal(f.session.getSnapshot().phase, 'editing');
    assert.equal(f.session.getSnapshot().current?.relay_url, raw);
    assert.equal(f.session.getSnapshot().scope, definitionScopeKey(f.scope));
    f.session.edit(f.review.proposed.persona, f.review.proposed.rules);
    await f.session.preview();
    assert.equal(f.session.getSnapshot().phase, 'review');
    const approval = f.session.confirmation();
    assert.ok(approval);
    await f.session.apply(approval);
    assert.equal(f.session.getSnapshot().receipt?.outcome, 'Confirmed');
    await f.session.reconcile();
    assert.equal(f.session.getSnapshot().receipt?.current?.definition_sha256, 'next');
    const cold = new DefinitionSession(f.port);
    cold.activate(f.scope);
    const before = f.calls.length;
    await cold.load();
    assert.equal(cold.getSnapshot().phase, 'held');
    assert.equal(cold.getSnapshot().receipt?.receipt_id, 'receipt');
    assert.deepEqual(
      f.calls.slice(before).map((command) => command.op),
      ['agent_definition_reconcile'],
    );
    assert.equal(f.calls.filter((command) => command.op === 'agent_definition_update').length, 1);
    assert.equal(f.account.relay_url, raw);
  });
}

for (const foreign of [
  'wss://Relay.Example:443',
  'wss://relay.example:443/',
  'wss://Relay.Example/',
  'https://relay.example',
  'https://other.example',
  'https://relay.example:444',
  'https://relay.example/prefix',
  'http://relay.example',
]) {
  test(`different raw DTO relay ${foreign} cannot enter read or held review`, async () => {
    const f = fixture('wss://Relay.Example:443/', foreign);
    await f.session.load();
    assert.equal(f.session.getSnapshot().phase, 'error');
    assert.equal(f.session.getSnapshot().current, null);
    assert.equal(f.session.confirmation(), null);
    f.session.activate(f.scope);
    f.handle(async () => f.receipt);
    await f.session.load();
    assert.equal(f.session.getSnapshot().phase, 'error');
    assert.equal(f.session.getSnapshot().receipt, null);
    assert.equal(
      f.calls.some((command) => command.op === 'agent_definition_update'),
      false,
    );
  });
}

test('canonical-equivalent DTO substitutions cannot enter review or mark an update Confirmed', async () => {
  const f = fixture('wss://Relay.Example:443/');
  const foreign = {
    ...f.review,
    current: { ...f.review.current, relay_url: 'https://relay.example' },
    proposed: { ...f.review.proposed, relay_url: 'https://relay.example' },
  };
  await f.session.load();
  f.session.edit(f.review.proposed.persona, f.review.proposed.rules);
  f.handle(async () => foreign);
  await f.session.preview();
  assert.equal(f.session.getSnapshot().phase, 'editing');
  assert.equal(f.session.getSnapshot().error, 'preview');
  assert.equal(f.session.confirmation(), null);
  assert.equal(
    f.calls.some((command) => command.op === 'agent_definition_update'),
    false,
  );

  const g = fixture('wss://Relay.Example:443/');
  await g.session.load();
  g.session.edit(g.review.proposed.persona, g.review.proposed.rules);
  await g.session.preview();
  const approval = g.session.confirmation();
  assert.ok(approval);
  g.handle(async () => ({ ...g.receipt, review: foreign, current: foreign.proposed }));
  await g.session.apply(approval);
  assert.equal(g.session.getSnapshot().phase, 'held');
  assert.equal(g.session.getSnapshot().receipt, null);
  assert.equal(g.session.getSnapshot().current, null);
  assert.equal(g.session.getSnapshot().receiptKnown, false);
  assert.equal(g.session.confirmation(), null);
});

for (const action of ['read', 'preview', 'update', 'reconcile', 'dismiss'] as const) {
  test(`${action} still fences raw configuration swaps with the same canonical relay during await`, async () => {
    const f = fixture('https://relay.example/');
    let resolve!: (value: unknown) => void;
    f.handle(
      () =>
        new Promise((yes) => {
          resolve = yes;
        }),
    );
    const pending =
      action === 'read'
        ? f.port.read(f.scope)
        : action === 'preview'
          ? f.port.preview(f.scope, f.baseline, f.review.proposed.persona, f.review.proposed.rules)
          : action === 'update'
            ? f.port.update(f.scope, f.review)
            : action === 'reconcile'
              ? f.port.reconcile(f.scope)
              : f.port.dismiss(f.scope, f.receipt);
    f.account.relay_url = 'wss://relay.example';
    resolve(
      action === 'read'
        ? f.baseline
        : action === 'preview'
          ? f.review
          : action === 'dismiss'
            ? true
            : f.receipt,
    );
    await assert.rejects(pending, /Account session changed/);
    assert.equal(f.calls.length, 1);
    assert.notEqual(
      definitionScopeKey(f.scope),
      definitionScopeKey({ ...f.scope, relay: f.account.relay_url }),
    );
  });
}
