import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, nodes, text, settle } from './tool-review-ui.fixture.ts';

test('compiled native review button directly requests one inert intent, never approval or action', async () => {
  for (const locale of ['en', 'pt-BR'] as const) {
    const f = fixture(locale);
    await f.session.refresh();
    const tree = f.view();
    const button = nodes(tree).find((node) => node.type === 'NativeButton')!;
    assert.equal(
      button.props.label,
      locale === 'en' ? 'Request a new review' : 'Solicitar nova revisão',
    );
    assert.ok(
      text(tree).includes(
        locale === 'en' ? 'approval period ended' : 'prazo da aprovação terminou',
      ),
    );
    button.props.onPress();
    await settle();
    assert.deepEqual(f.writes, [
      {
        op: 'decide_tool_request',
        request_id: 'old-request',
        observed_content_hash: 'ab'.repeat(32),
        decision: 'RequestReview',
      },
    ]);
    assert.ok(
      text(f.view()).includes(locale === 'en' ? 'New review requested' : 'Nova revisão solicitada'),
    );
    assert.ok(nodes(f.view()).every((node) => node.props.label !== button.props.label));
  }
});

test('compiled successor button rechecks current scope before opening the exact new request', async () => {
  const f = fixture();
  const rows = f.base();
  rows[0].tool_approval.review.state = 'Superseded';
  rows[0].tool_approval.review.successor_request_id = 'new-request';
  f.rows(rows);
  await f.session.refresh();
  const button = nodes(f.view()).find((node) => node.props.label === 'Open the new request')!;
  button.props.onPress();
  assert.equal(f.routes[0].params.id, 'new-request');
  f.state.snapshot.account.device_id = 'replacement';
  button.props.onPress();
  assert.equal(f.routes.length, 1);
});

test('actual compiled hook retires preflight reads on background/account and reloads only on focus/foreground', async () => {
  const f = fixture();
  const hook = f.hook();
  await settle();
  assert.ok(hook.session.ticket());
  let finish!: (value: any[]) => void;
  f.reads(
    () =>
      new Promise<any[]>((yes) => {
        finish = yes;
      }),
  );
  const attempt = hook.session.renew(hook.session.ticket());
  for (const fn of f.foreground) fn('background');
  finish(f.base());
  assert.equal(await attempt, false);
  assert.equal(f.writes.length, 0);
  f.reads(async () => f.base());
  for (const fn of f.foreground) fn('active');
  await settle();
  assert.ok(hook.session.ticket());
  f.state.snapshot.account.identity_id = 'different-owner';
  for (const fn of f.listeners) fn();
  assert.equal(hook.session.ticket(), null);
  f.close();
  assert.equal(f.foreground.size, 0);
  assert.equal(f.listeners.size, 0);
  assert.equal(f.canonical.size, 0);
  assert.equal(f.preflight.size, 0);
});

test('actual React compiler updates a stable session from cold null through eligibility and successor', async () => {
  const f = fixture();
  assert.ok(nodes(f.view()).every((n) => n.props.label !== 'Request a new review'));
  await f.session.refresh();
  assert.ok(nodes(f.view()).some((n) => n.props.label === 'Request a new review'));
  const prior = f.session.getSnapshot();
  const rows = f.base();
  rows[0].tool_approval.review.state = 'Superseded';
  rows[0].tool_approval.review.successor_request_id = 'next';
  f.rows(rows);
  await f.session.refresh();
  assert.equal(f.session.ticket(prior), null);
  assert.equal(f.session.successor(prior), null);
  assert.ok(nodes(f.view()).some((n) => n.props.label === 'Open the new request'));
  assert.ok(nodes(f.view()).every((n) => n.props.label !== 'Request a new review'));
});

test('focused foreground expiry performs one read-only metadata refresh; inactive or swapped scope cancels it', async (t) => {
  for (const retire of ['none', 'background', 'account', 'blur']) {
    t.mock.restoreAll();
    const f = fixture();
    t.mock.method(Date, 'now', () => f.clock());
    const rows = f.base();
    rows[0].tool_approval.expires_ms = String(f.clock() + 1000);
    rows[0].tool_approval.review.state = 'Unavailable';
    f.rows(rows);
    let reads = 0;
    f.reads(async () => {
      ++reads;
      const current = structuredClone(rows);
      if (f.clock() >= Number(current[0].tool_approval.expires_ms))
        current[0].tool_approval.review.state = 'Available';
      return current;
    });
    f.hook();
    await settle();
    assert.equal(reads, 1);
    assert.equal(f.timerCount(), 1);
    assert.equal(f.view(), null);
    if (retire === 'background') for (const fn of f.foreground) fn('background');
    if (retire === 'account') {
      f.state.snapshot.account.device_id = 'replacement';
      for (const fn of f.listeners) fn();
    }
    if (retire === 'blur') f.close();
    await f.advance(1001);
    assert.equal(reads, retire === 'none' ? 2 : 1);
    assert.equal(f.writes.length, 0);
    assert.equal(f.timerCount(), 0);
    if (retire === 'none')
      assert.ok(nodes(f.view()).some((n) => n.props.label === 'Request a new review'));
    f.close();
  }
});

test('compiled approved card distinguishes historical approval from expired/requested/superseded/reconcile state', async () => {
  for (const state of ['Available', 'Requested', 'Superseded', 'Reconcile']) {
    const f = fixture();
    const rows = f.base();
    rows[0].tool_approval.review.state = state;
    if (state === 'Superseded') rows[0].tool_approval.review.successor_request_id = 'next';
    f.rows(rows);
    await f.session.refresh();
    const tree = f.card(rows[0], 'Approved once. Waiting for the reviewed action.');
    assert.ok(text(tree).includes('Previous decision: approved once.'));
    assert.ok(!text(tree).includes('Waiting for the reviewed action.'));
    assert.ok(nodes(tree).some((n) => n.type === 'ToolReviewRecovery'));
  }
});
