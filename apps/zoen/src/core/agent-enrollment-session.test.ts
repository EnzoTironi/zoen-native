import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  EnrollmentSession,
  enrollmentScope,
  enrollmentScopeKey,
  type EnrollmentPort,
  type EnrollmentReceipt,
  type EnrollmentReview,
  type EnrollmentScope,
} from '../agent-enrollment/session.ts';
import type { AccountDto } from './generated/AccountDto.ts';

const scope: EnrollmentScope = { owner: 'owner', device: 'phone', relay: 'https://relay.example' };
const review = (overrides: Partial<EnrollmentReview> = {}): EnrollmentReview => ({
  agent: 'agent',
  digest: 'digest',
  name: 'Booking helper',
  handle: 'booking',
  persona: 'Helps plan your trips.',
  rules: ['Ask before booking.'],
  pairing: '123456',
  expires: 2000n,
  ...overrides,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture() {
  const calls: {
    action: string;
    code?: string;
    review?: EnrollmentReview;
    scope: EnrollmentScope;
  }[] = [];
  let now = 1000n;
  const port: EnrollmentPort = {
    preview: async (code, scope) => {
      calls.push({ action: 'preview', code, scope });
      return review();
    },
    approve: async (code, review, scope) => {
      calls.push({ action: 'approve', code, review, scope });
      return { receipt: 'receipt', agent: review.agent, digest: review.digest, outcome: 'Sent' };
    },
    receipt: async () => null,
    dismiss: async () => true,
    registration: async (review, scope) => {
      calls.push({ action: 'registered', review, scope });
      return null;
    },
  };
  const session = new EnrollmentSession(port, () => now);
  session.activate(scope);
  return { session, port, calls, time: (value: bigint) => (now = value) };
}
async function ready(f: ReturnType<typeof fixture>) {
  f.session.setCode('  offer-code  ');
  await f.session.check();
  return f.session.confirmation()!;
}

test('only a registered unlocked account can enter; device and relay are part of the UI scope', () => {
  const account: AccountDto = {
    identity_id: scope.owner,
    device_id: scope.device,
    relay_url: scope.relay,
    name: 'Owner',
    handle: 'owner',
    registered: true,
    unlocked: true,
    setup_pending: false,
  };
  assert.deepEqual(enrollmentScope('account', account), scope);
  assert.equal(enrollmentScope('demo', account), null);
  assert.equal(enrollmentScope('account', null), null);
  for (const patch of [{ unlocked: false }, { registered: false }, { setup_pending: true }])
    assert.equal(enrollmentScope('account', { ...account, ...patch }), null);
  for (const patch of [{ owner: 'other' }, { device: 'other' }, { relay: 'https://other.example' }])
    assert.notEqual(enrollmentScopeKey({ ...scope, ...patch }), enrollmentScopeKey(scope));
});

test('input and preview never authorize; the visible review is immutable', async () => {
  const f = fixture();
  const offer = review();
  f.port.preview = async () => offer;
  f.session.setCode('code');
  assert.equal(f.calls.length, 0);
  await f.session.check();
  assert.equal(f.session.getSnapshot().phase, 'review');
  assert.equal(f.calls.filter((call) => call.action === 'approve').length, 0);
  assert.notEqual(f.session.getSnapshot().review, offer);
  assert.ok(Object.isFrozen(f.session.getSnapshot().review));
  assert.ok(Object.isFrozen(f.session.getSnapshot().review!.rules));
});

test('replacing input retires the prior preview and late scanner ticket', async () => {
  const f = fixture();
  const pending = deferred<EnrollmentReview>();
  f.port.preview = () => pending.promise;
  const scanner = f.session.inputTicket()!;
  f.session.setCode('old');
  const check = f.session.check();
  f.session.setCode('new');
  pending.resolve(review({ name: 'Old profile' }));
  await check;
  assert.equal(f.session.getSnapshot().code, 'new');
  assert.equal(f.session.getSnapshot().review, null);
  assert.equal(f.session.setCode('late scan', scanner), false);
});

for (const changed of [
  null,
  { ...scope, owner: 'other' },
  { ...scope, device: 'other' },
  { ...scope, relay: 'https://other.example' },
]) {
  test(`preview result cannot publish after retirement or changed scope ${enrollmentScopeKey(changed)}`, async () => {
    const f = fixture();
    const pending = deferred<EnrollmentReview>();
    f.port.preview = () => pending.promise;
    const ticket = f.session.inputTicket()!;
    f.session.setCode('code');
    const check = f.session.check();
    if (changed) f.session.activate(changed);
    else f.session.retire();
    pending.resolve(review());
    await check;
    assert.equal(f.session.getSnapshot().review, null);
    assert.equal(f.session.getSnapshot().code, '');
    assert.equal(f.session.setCode('late scan', ticket), false);
    assert.equal(f.calls.filter((call) => call.action === 'approve').length, 0);
  });
}

test('only the displayed digest and pairing confirmation admits one approval', async () => {
  const f = fixture();
  const action = await ready(f);
  await f.session.approve({ ...action, digest: 'changed' });
  await f.session.approve({ ...action, pairing: '654321' });
  assert.equal(f.calls.filter((call) => call.action === 'approve').length, 0);
  const pending = deferred<{ receipt: string; agent: string; digest: string; outcome: 'Sent' }>();
  f.port.approve = async (code, review, scope) => {
    f.calls.push({ action: 'approve', code, review, scope });
    return pending.promise;
  };
  const first = f.session.approve(action);
  await f.session.approve(action);
  assert.equal(f.calls.filter((call) => call.action === 'approve').length, 1);
  assert.equal(f.session.getSnapshot().phase, 'submitting');
  assert.equal(f.session.getSnapshot().code, '');
  assert.equal(f.session.setCode('other-code'), false);
  assert.deepEqual(f.calls.at(-1), {
    action: 'approve',
    code: 'offer-code',
    review: f.session.getSnapshot().review,
    scope,
  });
  pending.resolve({ receipt: 'receipt', agent: 'agent', digest: 'digest', outcome: 'Sent' });
  await first;
  assert.equal(f.session.getSnapshot().phase, 'waiting');
  assert.equal(f.session.getSnapshot().outcome, 'Sent');
});

test('expired offer and callbacks from a previous visible scope cannot authorize', async () => {
  const f = fixture();
  const old = await ready(f);
  f.session.activate({ ...scope, device: 'new-phone' });
  const current = await ready(f);
  await f.session.approve(old);
  assert.equal(f.calls.filter((call) => call.action === 'approve').length, 0);
  f.time(2000n);
  await f.session.approve(current);
  assert.equal(f.session.getSnapshot().error, 'expired');
  assert.equal(f.calls.filter((call) => call.action === 'approve').length, 0);
});

for (const outcome of ['throw', 'mismatched-agent', 'mismatched-digest', 'Unknown'] as const) {
  test(`uncertain approval ${outcome} never exposes a resend path`, async () => {
    const f = fixture();
    const action = await ready(f);
    let approvals = 0;
    f.port.approve = async () => {
      approvals++;
      if (outcome === 'throw') throw Error('connection closed after dispatch');
      return {
        receipt: 'receipt',
        agent: outcome === 'mismatched-agent' ? 'other' : 'agent',
        digest: outcome === 'mismatched-digest' ? 'other' : 'digest',
        outcome: outcome === 'Unknown' ? 'Unknown' : 'Sent',
      };
    };
    await f.session.approve(action);
    assert.equal(f.session.getSnapshot().outcome, 'Unknown');
    assert.equal(f.session.getSnapshot().phase, 'waiting');
    assert.equal(f.session.getSnapshot().code, '');
    assert.equal(f.session.inputTicket(), null);
    assert.equal(f.session.confirmation(), null);
    assert.equal(f.session.setCode('same-code'), false);
    await f.session.check();
    await f.session.approve(action);
    await f.session.reconcile();
    assert.equal(approvals, 1);
    assert.equal(f.session.getSnapshot().phase, 'waiting');
  });
}

test('closing while approval is in flight clears local code and cannot publish to a new flow', async () => {
  const f = fixture();
  const action = await ready(f);
  const pending = deferred<{ receipt: string; agent: string; digest: string; outcome: 'Sent' }>();
  f.port.approve = () => pending.promise;
  const send = f.session.approve(action);
  f.session.retire();
  f.session.activate(scope);
  f.session.setCode('new input');
  pending.resolve({ receipt: 'receipt', agent: 'agent', digest: 'digest', outcome: 'Sent' });
  await send;
  assert.equal(f.session.getSnapshot().code, 'new input');
  assert.equal(f.session.getSnapshot().outcome, null);
  assert.equal(f.session.getSnapshot().phase, 'entry');
});

test('restoring a public delivery receipt only resumes status, never approval or old code', async () => {
  const f = fixture();
  const receipt: EnrollmentReceipt = { review: review(), receipt: 'receipt', outcome: 'Unknown' };
  assert.equal(f.session.restore(receipt, f.session.inputTicket()!), true);
  assert.equal(f.session.getSnapshot().code, '');
  assert.equal(f.session.confirmation(), null);
  await f.session.check();
  await f.session.reconcile();
  assert.deepEqual(
    f.calls.map((call) => call.action),
    ['registered'],
  );
  assert.equal(f.session.getSnapshot().phase, 'waiting');
  f.port.registration = async () => 'Active';
  await f.session.reconcile();
  assert.equal(f.session.getSnapshot().phase, 'registered');
  assert.equal(f.session.getSnapshot().outcome, 'Unknown');
});

test('late receipt cannot replace concurrent input, a newer preview or another account', async () => {
  const f = fixture();
  const receipt: EnrollmentReceipt = { review: review(), receipt: 'receipt', outcome: 'Sent' };
  const ticket = f.session.inputTicket()!;
  f.session.setCode('typing');
  assert.equal(f.session.restore(receipt, ticket), false);
  assert.equal(f.session.getSnapshot().code, 'typing');
  const current = f.session.inputTicket()!;
  f.session.activate({ ...scope, owner: 'other' });
  assert.equal(f.session.restore(receipt, current), false);
  assert.equal(f.session.getSnapshot().review, null);
});

test('status failure remains actionable and stale status cannot publish after retirement', async () => {
  const f = fixture();
  f.session.restore(
    { review: review(), receipt: 'receipt', outcome: 'Sent' },
    f.session.inputTicket()!,
  );
  f.port.registration = async () => {
    throw Error('offline');
  };
  await f.session.reconcile();
  assert.equal(f.session.getSnapshot().phase, 'waiting');
  assert.equal(f.session.getSnapshot().error, 'status');
  const pending = deferred<'Active'>();
  f.port.registration = () => pending.promise;
  const check = f.session.reconcile();
  assert.equal(f.session.getSnapshot().reconciling, true);
  f.session.retire();
  pending.resolve('Active');
  await check;
  assert.equal(f.session.getSnapshot().phase, 'entry');
  assert.equal(f.session.getSnapshot().review, null);
});

test('unknown delivery recovers its public receipt without dispatching approval again', async () => {
  const f = fixture();
  const action = await ready(f);
  f.port.approve = async () => {
    throw Error('connection lost');
  };
  await f.session.approve(action);
  assert.equal(f.session.restartConfirmation(), null);
  f.port.receipt = async () => ({
    review: review(),
    receipt: 'persisted-receipt',
    outcome: 'Unknown',
  });
  await f.session.reconcile();
  assert.equal(f.session.getSnapshot().receipt, 'persisted-receipt');
  assert.equal(f.session.getSnapshot().receiptKnown, true);
  assert.equal(f.session.confirmation(), null);
});

test('starting another connection is explicit CAS dismissal; a late dialog cannot clear a newer receipt', async () => {
  const f = fixture();
  f.session.restore(
    { review: review(), receipt: 'receipt-1', outcome: 'Unknown' },
    f.session.inputTicket()!,
  );
  const oldDialog = f.session.restartConfirmation()!;
  f.session.retire();
  f.session.activate(scope);
  f.session.restore(
    { review: review(), receipt: 'receipt-2', outcome: 'Sent' },
    f.session.inputTicket()!,
  );
  let dismissals = 0;
  f.port.dismiss = async (receipt, reviewed, current) => {
    dismissals++;
    assert.equal(receipt, 'receipt-2');
    assert.equal(reviewed.digest, 'digest');
    assert.deepEqual(current, scope);
    return true;
  };
  await f.session.startAnother(oldDialog);
  assert.equal(dismissals, 0);
  await f.session.startAnother(f.session.restartConfirmation()!);
  assert.equal(dismissals, 1);
  assert.equal(f.session.getSnapshot().phase, 'entry');
  assert.equal(f.session.getSnapshot().code, '');
  assert.equal(f.session.getSnapshot().receipt, null);
});

test('failed CAS preserves the receipt and late dismissal cannot reset a newer flow', async () => {
  const f = fixture();
  f.session.restore(
    { review: review(), receipt: 'receipt', outcome: 'Sent' },
    f.session.inputTicket()!,
  );
  f.port.dismiss = async () => {
    throw Error('CAS refused');
  };
  await f.session.startAnother(f.session.restartConfirmation()!);
  assert.equal(f.session.getSnapshot().receipt, 'receipt');
  assert.equal(f.session.getSnapshot().error, 'dismiss');
  const pending = deferred<boolean>();
  f.port.dismiss = () => pending.promise;
  const task = f.session.startAnother(f.session.restartConfirmation()!);
  f.session.retire();
  f.session.activate(scope);
  f.session.setCode('new-code');
  pending.resolve(true);
  await task;
  assert.equal(f.session.getSnapshot().code, 'new-code');
});

test('a registered Agent revoked after delivery stops waiting without enabling approval again', async () => {
  const f = fixture();
  f.session.restore(
    { review: review(), receipt: 'receipt', outcome: 'Sent' },
    f.session.inputTicket()!,
  );
  f.port.registration = async () => 'Revoked';
  await f.session.reconcile();
  assert.equal(f.session.getSnapshot().phase, 'revoked');
  assert.equal(f.session.confirmation(), null);
  assert.equal(f.session.getSnapshot().code, '');
  assert.ok(f.session.restartConfirmation());
});

test('a false dismissal CAS with a newer held receipt cannot clear or replace this flow', async () => {
  const f = fixture();
  f.session.restore(
    { review: review(), receipt: 'old', outcome: 'Unknown' },
    f.session.inputTicket()!,
  );
  f.port.dismiss = async () => false;
  f.port.receipt = async () => ({
    review: review({ digest: 'new' }),
    receipt: 'new',
    outcome: 'Unknown',
  });
  await f.session.startAnother(f.session.restartConfirmation()!);
  assert.equal(f.session.getSnapshot().receipt, 'old');
  assert.equal(f.session.getSnapshot().phase, 'waiting');
  assert.equal(f.session.getSnapshot().error, 'dismiss');
});
