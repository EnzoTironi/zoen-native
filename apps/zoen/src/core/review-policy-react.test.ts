import assert from 'node:assert/strict';
import { test } from 'node:test';
import { reviewPolicyReactFixture } from './review-policy-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { OwnerPolicyOptionsStatus } from './generated/OwnerPolicyOptionsStatus';
import { createOwnerPolicyPort, policyAvailability } from '../owner-policy/core-port.ts';
import { policyScopeKey } from '../owner-policy/session.ts';

for (const change of [undefined, 'identity', 'device', 'relay'] as const) {
  test(`compiled tool review resumes canonical metadata after real cold reopen: ${change ?? 'same IDs/new epoch'}`, async () => {
    const f = await reviewPolicyReactFixture('review');
    try {
      await f.render();
      assert.equal(f.review().state.phase, 'ready');
      const ticket = f.review().session.ticket(f.review().state);
      assert.ok(ticket);
      const initial = f.calls.filter((c) => c.op === 'requests').length;
      await f.reopen(change);
      assert.equal(f.review().state.phase, 'ready');
      assert.ok(f.calls.filter((c) => c.op === 'requests').length > initial);
      const current = f.review().session.ticket(f.review().state);
      assert.ok(current);
      assert.notEqual(current.scope, ticket.scope);
      const reads = f.calls.length;
      await f.act(() => f.review().session.renew(ticket));
      assert.equal(f.calls.length, reads, 'retired button cannot read or dispatch');
      assert.equal(f.calls.filter((c) => c.op === 'decide_tool_request').length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
  test(`compiled account-policy presentation follows real cold reopen: ${change ?? 'same IDs/new epoch'}`, async () => {
    const f = await reviewPolicyReactFixture('policy');
    try {
      await f.render();
      assert.equal(f.policy().state.phase, 'held');
      assert.equal(f.policy().state.scope, f.expectedPolicyScope());
      const previous = f.policy().state;
      await f.reopen(change);
      assert.equal(f.policy().state.phase, 'held');
      assert.equal(f.policy().state.scope, f.expectedPolicyScope());
      assert.notEqual(f.policy().state.scope, previous.scope);
      const inquiries = f.calls.filter((c) => c.op === 'request_account_policy_options').length;
      await f.act(() => f.policy().session.refresh(previous));
      await f.act(() => f.policy().session.retryOptions(previous));
      assert.equal(
        f.calls.filter((c) => c.op === 'request_account_policy_options').length,
        inquiries,
      );
      await f.act(() => f.policy().session.refresh());
      assert.equal(
        f.calls.filter((c) => c.op === 'request_account_policy_options').length,
        inquiries,
        'uncertain inquiry is reconciled, never automatically resubmitted',
      );
      assert.equal(f.policy().state.context, null);
      assert.equal(f.policy().state.receipt, null);
      assert.equal(f.calls.filter((c) => c.op === 'submit_owner_policy').length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
}

test('compiled tool review discards old metadata resolving after same-ID cold reopen', async () => {
  const f = await reviewPolicyReactFixture('review'),
    gate = deferred<AgentRequestDto[]>();
  try {
    f.read(gate.promise);
    await f.render();
    assert.equal(f.review().state.phase, 'loading');
    await f.reopen();
    assert.equal(f.review().state.phase, 'ready');
    const current = f.review().state;
    await f.act(() => gate.resolve([{ ...f.request, title: 'Obsolete response' }]));
    assert.equal(f.review().state, current);
    assert.equal(f.review().state.request?.title, 'Synthetic request');
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve([]);
    await f.close();
  }
});

test('compiled policy hook rejects late original inquiry and retains fresh ambiguous state', async () => {
  const f = await reviewPolicyReactFixture('policy'),
    gate = deferred<OwnerPolicyOptionsStatus>();
  try {
    f.inquire(gate.promise);
    await f.render();
    assert.equal(f.policy().state.phase, 'loading');
    await f.reopen('device');
    assert.equal(f.policy().state.phase, 'held');
    const current = f.policy().state;
    await f.act(() =>
      gate.resolve({
        source: 'obsolete-query',
        inquiry_digest: 'a'.repeat(64),
        state: 'pending',
        context: null,
      }),
    );
    assert.equal(f.policy().state, current);
    assert.equal(f.policy().state.optionsSource, null);
    assert.equal(f.policy().state.context, null);
    assert.equal(f.calls.filter((c) => c.op === 'submit_owner_policy').length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    gate.reject(Error('fixture cleanup'));
    await f.close();
  }
});

for (const kind of ['review', 'policy'] as const)
  test(`installed focus/AppState retires ${kind} callbacks and reactivates current scope`, async () => {
    const f = await reviewPolicyReactFixture(kind);
    try {
      await f.render();
      const ticket = kind === 'review' ? f.review().session.ticket(f.review().state) : null;
      const previous = kind === 'policy' ? f.policy().state : null;
      for (const event of ['background', 'blur'] as const) {
        await f.act(() => (event === 'background' ? f.foreground('background') : f.focus(false)));
        assert.equal(
          kind === 'review' ? f.review().state.phase : f.policy().state.phase,
          kind === 'review' ? 'inactive' : 'unavailable',
        );
        await f.act(() => (event === 'background' ? f.foreground('active') : f.focus(true)));
        assert.equal(
          kind === 'review' ? f.review().state.phase : f.policy().state.phase,
          kind === 'review' ? 'ready' : 'held',
        );
      }
      const before = f.calls.length;
      if (kind === 'review') await f.act(() => f.review().session.renew(ticket));
      else await f.act(() => f.policy().session.refresh(previous!));
      assert.equal(f.calls.length, before);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('policy render availability follows canonical private-DM changes while callback port stays live', async () => {
  const f = await reviewPolicyReactFixture('policy');
  try {
    await f.render();
    const observed = f.store.getSnapshot(),
      scope = policyAvailability(f.store, f.target).scope;
    assert.ok(scope);
    const port = createOwnerPolicyPort(f.store, f.target);
    f.snapshot.conversations[0]!.kind = 'Group';
    await f.act(() => f.store.refresh());
    assert.equal(f.policy().unavailableReason, 'private-conversation-required');
    assert.equal(f.policy().state.scope, '');
    // Explicit snapshot is presentation only. Default imperative reads still see current state.
    assert.equal(
      policyScopeKey(policyAvailability(f.store, f.target, observed).scope),
      policyScopeKey(scope),
    );
    const count = f.calls.length;
    await assert.rejects(port.inquire(scope), /session changed/);
    assert.equal(f.calls.length, count);
    f.snapshot.conversations[0]!.kind = 'Direct';
    await f.act(() => f.store.refresh());
    assert.equal(f.policy().unavailableReason, null);
    assert.equal(f.policy().state.scope, f.expectedPolicyScope());
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
