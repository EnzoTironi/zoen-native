import assert from 'node:assert/strict';
import test from 'node:test';
import type { OwnerPolicySession, PolicyPort } from '../owner-policy/session.ts';
import {
  request,
  policy,
  scope,
  context,
  review,
  settle,
  deferred,
  fixture,
  screen,
  button,
  has,
  fill,
  prepare,
} from './owner-policy-screen-fixture.ts';

type Review = Awaited<ReturnType<PolicyPort['prepare']>>;

test('compiled form sends exact draft and review submits the actual confirmation ticket once', async () => {
  const f = await fixture(),
    render = screen(f);
  fill(render, '9,223,372,036,854.775807', '0.000100');
  const action = button(render(), 'ownerPolicy.review');
  assert.equal(action.props.disabled, false);
  action.props.onPress();
  action.props.onPress();
  await settle();
  assert.equal(f.f.prepares.length, 1);
  assert.deepEqual(f.f.prepares[0]!.slice(3), ['9223372036854775807', '100', true]);
  const expected = f.session.confirmation();
  assert.ok(expected);
  const tickets: Parameters<OwnerPolicySession['submit']>[0][] = [];
  const submit = f.session.submit.bind(f.session);
  f.session.submit = async (ticket) => {
    tickets.push(ticket);
    return submit(ticket);
  };
  const confirm = button(render(), 'ownerPolicy.confirm');
  assert.equal(confirm.props.disabled, false);
  confirm.props.onPress();
  confirm.props.onPress();
  await settle();
  assert.deepEqual(tickets, [expected]);
  assert.equal(f.f.submissions.length, 1);
  assert.equal(f.f.submissions[0]!.canonical_policy_json, '{"exact":"signed Core policy"}');
  assert.equal(f.session.getSnapshot().phase, 'pending');
  assert.equal(has(render(), 'ownerPolicy.installed'), false);
});

for (const change of ['owner', 'refocus', 'quote'] as const)
  test(`old compiled form callback cannot prepare after ${change} changes observed state`, async () => {
    const f = await fixture(),
      render = screen(f);
    fill(render);
    const old = button(render(), 'ownerPolicy.review');
    if (change === 'quote') {
      f.f.context = context({ options_digest: '1'.repeat(64) });
      f.session.invalidate();
      await f.session.reconcile();
      await settle();
    } else {
      f.session.retire();
      if (change === 'owner') f.f.owner = { ...scope, identity: 'next-owner', epoch: 2 };
      f.session.activate(f.f.owner);
      await f.session.refresh();
      await settle();
    }
    old.props.onPress();
    await settle();
    assert.equal(f.f.prepares.length, 0);
    fill(render);
    button(render(), 'ownerPolicy.review').props.onPress();
    await settle();
    assert.equal(f.f.prepares.length, 1);
    assert.deepEqual(f.f.prepares[0]![0], f.f.owner);
    assert.equal(f.f.prepares[0]![2], f.f.context.options_digest);
  });

test('invalid or busy compiled form cannot call prepare through its rendered onPress', async () => {
  const f = await fixture(),
    render = screen(f);
  for (const [limit, attempt] of [
    ['', '0'],
    ['1e3', '0'],
    ['-1', '0'],
    ['1', '2'],
    ['1', '0.000099'],
  ]) {
    fill(render, limit, attempt);
    const action = button(render(), 'ownerPolicy.review');
    assert.equal(action.props.disabled, true);
    action.props.onPress();
    await settle();
    assert.equal(f.f.prepares.length, 0);
  }
  fill(render);
  const gate = deferred<Review>();
  f.port.prepare = async (...args) => {
    f.f.prepares.push(args);
    return gate.promise;
  };
  button(render(), 'ownerPolicy.review').props.onPress();
  assert.equal(f.session.getSnapshot().phase, 'preparing');
  assert.equal(
    render().some((node) => node.type === 'Field'),
    false,
  );
  gate.resolve(review(scope, '1000000', '100', true));
  await settle();
  assert.equal(f.f.prepares.length, 1);
  assert.equal(f.session.getSnapshot().phase, 'review');
});

for (const outcome of ['pending', 'held'] as const)
  test(`compiled ${outcome} UI only reconciles the original without resubmission`, async () => {
    const f = await fixture(),
      render = screen(f);
    await prepare(f, render);
    if (outcome === 'held') {
      f.port.submit = async (_owner, value) => {
        f.f.submissions.push(value);
        throw Error('lost after enqueue');
      };
      f.port.find = async () => {
        f.f.finds++;
        return null;
      };
    }
    const oldConfirm = button(render(), 'ownerPolicy.confirm');
    oldConfirm.props.onPress();
    await settle();
    assert.equal(f.session.getSnapshot().phase, outcome);
    for (let i = 0; i < 2; i++) {
      const tree = render();
      assert.equal(
        tree.some((node) => node.type === 'Field'),
        false,
      );
      assert.equal(
        tree.some(
          (node) =>
            node.type === 'NativeButton' &&
            ['ownerPolicy.confirm', 'ownerPolicy.review', 'ownerPolicy.edit'].includes(
              node.props.label,
            ),
        ),
        false,
      );
      button(tree, 'ownerPolicy.checkStatus').props.onPress();
      oldConfirm.props.onPress();
      await settle();
    }
    assert.equal(f.f.prepares.length, 1);
    assert.equal(f.f.submissions.length, 1);
    assert.equal(has(render(), 'ownerPolicy.installed'), false);
  });

for (const [effect, patch, status] of [
  ['unknown', {}, 'ownerPolicy.historical'],
  ['superseded', { current_policy_digest: 'f'.repeat(64) }, 'ownerPolicy.superseded'],
  ['expired', { current_expired: true }, 'ownerPolicy.expired'],
  ['disabled', { current_enabled: false }, 'ownerPolicy.disabled'],
  ['current', {}, 'ownerPolicy.installed'],
] as const)
  test(`compiled installed receipt renders ${effect} without an active-budget false claim`, async () => {
    const f = await fixture(),
      render = screen(f);
    await prepare(f, render);
    f.f.receipt = { ...f.f.receipt, state: 'installed' };
    if (effect !== 'unknown')
      f.f.context = context({
        current_policy_digest: policy,
        current_limit_units: '1000000',
        current_max_attempt_units: '100',
        current_enabled: true,
        current_expired: false,
        current_expires_at_ms: '200000',
        existing_profile_count: 1,
        ...patch,
      });
    button(render(), 'ownerPolicy.confirm').props.onPress();
    await settle();
    assert.equal(f.session.getSnapshot().phase, 'installed');
    assert.equal(f.session.getSnapshot().effectiveness, effect);
    const tree = render();
    assert.equal(has(tree, status), true);
    assert.equal(has(tree, 'ownerPolicy.installed'), effect === 'current');
    assert.equal(
      tree.some(
        (node) => node.type === 'NativeButton' && node.props.label === 'ownerPolicy.confirm',
      ),
      false,
    );
  });

for (const outcome of ['refused', 'delivery_failed', 'expired'] as const)
  test(`compiled conclusive quote ${outcome} exposes one explicit inquiry without disabled Edit`, async () => {
    const f = await fixture(),
      render = screen(f);
    const terminal = {
      source: 'terminal-query',
      inquiry_digest: 'e'.repeat(64),
      state: outcome,
      context: null,
    };
    f.port.inquire = async () => {
      f.f.queries++;
      return terminal;
    };
    f.port.options = async () => terminal;
    await f.session.refresh();
    await settle();
    assert.equal(f.session.getSnapshot().optionsState, outcome);
    const tree = render();
    assert.equal(
      has(
        tree,
        outcome === 'expired' ? 'ownerPolicy.quoteExpired' : 'ownerPolicy.quoteUnavailable',
      ),
      true,
    );
    assert.equal(
      tree.some((node) => node.type === 'Field'),
      false,
    );
    assert.equal(
      tree.some((node) => node.type === 'NativeButton' && node.props.label === 'ownerPolicy.edit'),
      false,
    );
    const retry = button(tree, 'agents.tryAgain');
    assert.equal(retry.props.disabled, false);
    const before = f.f.queries,
      gate = deferred<Awaited<ReturnType<PolicyPort['inquire']>>>();
    f.port.inquire = async () => {
      f.f.queries++;
      return gate.promise;
    };
    retry.props.onPress();
    retry.props.onPress();
    assert.equal(f.f.queries, before + 1);
    assert.equal(
      render().some((node) => node.type === 'Loading'),
      true,
    );
    const ready = {
      source: 'fresh-quote',
      inquiry_digest: 'e'.repeat(64),
      state: 'ready',
      context: context(),
    };
    f.port.options = async () => ready;
    gate.resolve(ready);
    await settle();
    assert.equal(f.session.getSnapshot().phase, 'editing');
    retry.props.onPress();
    await settle();
    assert.equal(f.f.queries, before + 1);
    assert.equal(f.f.prepares.length, 0);
    assert.equal(f.f.submissions.length, 0);
  });

test('compiled pending quote and focus resume only read the original inquiry', async () => {
  const f = await fixture(),
    render = screen(f);
  const original = {
    source: 'pending-quote',
    inquiry_digest: 'e'.repeat(64),
    state: 'pending',
    context: null,
  };
  let reads = 0;
  f.port.inquire = async () => {
    f.f.queries++;
    return original;
  };
  f.port.options = async (_owner, source) => {
    reads++;
    assert.equal(source, original.source);
    return original;
  };
  await f.session.refresh();
  await settle();
  const before = f.f.queries;
  const tree = render();
  assert.equal(
    tree.some((node) => node.type === 'Loading'),
    true,
  );
  assert.equal(
    tree.some((node) => node.type === 'Field'),
    false,
  );
  assert.equal(
    tree.some((node) => node.type === 'NativeButton' && node.props.label === 'agents.tryAgain'),
    false,
  );
  button(tree, 'ownerPolicy.checkStatus').props.onPress();
  await settle();
  f.session.retire();
  f.session.activate(f.f.owner);
  await f.session.refresh();
  await settle();
  button(render(), 'ownerPolicy.checkStatus').props.onPress();
  await settle();
  assert.equal(f.f.queries, before);
  assert.ok(reads >= 3);
  assert.equal(f.session.getSnapshot().optionsSource, original.source);
  assert.equal(f.f.prepares.length, 0);
  assert.equal(f.f.submissions.length, 0);
});

test('old compiled confirmation cannot approve a later edited review', async () => {
  const f = await fixture(),
    render = screen(f);
  await prepare(f, render);
  const old = button(render(), 'ownerPolicy.confirm');
  f.session.edit(f.session.getSnapshot(), { limitInput: '2000000' });
  await f.session.prepare();
  await settle();
  assert.equal(f.session.getSnapshot().review?.limit_units, '2000000');
  old.props.onPress();
  await settle();
  assert.equal(f.f.submissions.length, 0);
  button(render(), 'ownerPolicy.confirm').props.onPress();
  await settle();
  assert.equal(f.f.submissions.length, 1);
  assert.equal(f.f.submissions[0]!.limit_units, '2000000');
});

test('compiled ambiguous inquiry offers status only and never dispatches another quote', async () => {
  const f = await fixture(),
    render = screen(f);
  let reads = 0;
  f.port.inquire = async () => {
    f.f.queries++;
    throw Error('unknown delivery');
  };
  f.port.findOptions = async () => {
    reads++;
    return null;
  };
  await f.session.refresh();
  await settle();
  const before = f.f.queries;
  const tree = render();
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(has(tree, 'ownerPolicy.uncertain'), true);
  assert.equal(
    tree.some((node) => node.type === 'NativeButton' && node.props.label === 'agents.tryAgain'),
    false,
  );
  button(tree, 'ownerPolicy.checkStatus').props.onPress();
  await settle();
  assert.equal(f.f.queries, before);
  assert.ok(reads >= 2);
  assert.equal(f.f.prepares.length, 0);
  assert.equal(f.f.submissions.length, 0);
});

test('compiled owner budget route uses actual image validation before opening the composed screen', async () => {
  const f = await fixture(),
    render = screen(f);
  assert.equal(
    render.route([request]).some((node) => node.type === 'Field'),
    true,
  );
  const tool = request.tool_approval!;
  for (const rows of [
    [],
    [{ ...request, agent: { ...request.agent, is_mine: false } }],
    [{ ...request, tool_approval: null }],
    [{ ...request, tool_approval: { ...tool, canonical_tool: 'other' } }],
    [{ ...request, tool_approval: { ...tool, image: { ...tool.image!, prompt: ' ' } } }],
  ]) {
    const tree = render.route(rows);
    assert.equal(
      tree.some((node) => node.type === 'Field'),
      false,
    );
    assert.equal(
      tree.some(
        (node) => node.type === 'EmptyState' && node.props.title === 'ownerPolicy.unavailable',
      ),
      true,
    );
  }
});

test('account management reuses the compiled native form and explicit disable confirmation without an image target', async () => {
  const f = await fixture();
  f.f.context = context({
    current_policy_digest: policy,
    current_limit_units: '1000000',
    current_max_attempt_units: '100',
    current_enabled: true,
    current_expired: false,
    current_expires_at_ms: '200000',
    existing_profile_count: 1,
  });
  f.f.owner = {
    identity: scope.identity,
    device: scope.device,
    relay: scope.relay,
    epoch: scope.epoch,
    purpose: 'account',
    spaceId: scope.spaceId,
    agentId: scope.agentId,
  };
  f.session.activate(f.f.owner);
  await f.session.refresh();
  await settle();
  const render = screen(f, 'account');
  fill(render);
  const toggle = render().find((node) => node.type === 'Switch');
  assert.ok(toggle);
  toggle.props.onValueChange(false);
  button(render(), 'ownerPolicy.review').props.onPress();
  await settle();
  assert.equal(f.f.prepares.length, 1);
  assert.equal(f.session.getSnapshot().review?.enabled, false);
  assert.equal(f.f.submissions.length, 0);
  button(render(), 'ownerPolicy.disable').props.onPress();
  await settle();
  assert.equal(f.f.submissions.length, 1);
  assert.equal(f.session.getSnapshot().phase, 'pending');
  assert.equal(has(render(), 'ownerPolicy.installed'), false);
});

test('group image budget action selects a private account context without passing group facts', async () => {
  const f = await fixture(),
    render = screen(f);
  const tree = render.route([request], [{ id: request.space_id, kind: 'Group' }]);
  const selector = tree.find((node) => node.type === 'AccountBudgetScreen');
  assert.ok(selector);
  assert.deepEqual(JSON.parse(JSON.stringify(selector.props)), { agentId: request.agent.id });
  assert.equal(
    tree.some((node) => node.type === 'Field'),
    false,
  );
});
