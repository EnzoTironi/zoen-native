import assert from 'node:assert/strict';
import test from 'node:test';
import {
  button,
  context,
  fill,
  fixture,
  has,
  policy,
  review as coreReview,
  scope,
  screen,
  settle,
} from './owner-policy-screen-fixture.ts';

async function account(current = context()) {
  const f = await fixture();
  f.f.context = current;
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
  return { ...f, render: screen(f, 'account') };
}
function toggle(render: ReturnType<typeof screen>, enabled: boolean) {
  const control = render().find((node) => node.type === 'Switch');
  assert.ok(control);
  control.props.onValueChange(enabled);
}

test('account without a policy can review explicit disable without typing unused limits', async () => {
  const f = await account();
  assert.equal(button(f.render(), 'ownerPolicy.review').props.disabled, true);
  toggle(f.render, false);
  const review = button(f.render(), 'ownerPolicy.review');
  assert.equal(review.props.disabled, false);
  review.props.onPress();
  await settle();
  assert.equal(f.f.prepares.length, 1);
  assert.deepEqual(f.f.prepares[0]!.slice(3), ['0', '0', false]);
  assert.equal(f.session.getSnapshot().phase, 'review');
  assert.equal(f.session.getSnapshot().review?.version, 1);
  assert.equal(
    f.session.getSnapshot().review?.allowed_profiles[0],
    f.f.context.image_profile_digest,
  );
  assert.equal(f.f.submissions.length, 0);
  button(f.render(), 'ownerPolicy.disable').props.onPress();
  await settle();
  assert.equal(f.f.submissions.length, 1);
  assert.equal(f.session.getSnapshot().phase, 'pending');
  assert.equal(has(f.render(), 'ownerPolicy.installed'), false);
});

test('turning spending back on preserves unsubmitted text and restores amount validation', async () => {
  const f = await account();
  fill(f.render, '1.', '0.000100');
  toggle(f.render, false);
  assert.equal(button(f.render(), 'ownerPolicy.review').props.disabled, false);
  toggle(f.render, true);
  const fields = f.render().filter((node) => node.type === 'Field');
  assert.equal(fields[0]!.props.value, '1.');
  assert.equal(fields[1]!.props.value, '0.000100');
  assert.equal(button(f.render(), 'ownerPolicy.review').props.disabled, true);
  fill(f.render, '2', '0.000101');
  button(f.render(), 'ownerPolicy.review').props.onPress();
  await settle();
  assert.deepEqual(f.f.prepares[0]!.slice(3), ['2000000', '101', true]);
  assert.equal(f.f.submissions.length, 0);
});

test('disable preserves exact authenticated existing limits instead of an invalid local draft', async () => {
  const f = await account(
    context({
      current_policy_digest: policy,
      current_limit_units: '9007199254740995',
      current_max_attempt_units: '9007199254740993',
      current_enabled: true,
      current_expired: false,
      current_expires_at_ms: '200000',
      existing_profile_count: 1,
    }),
  );
  fill(f.render, '-1', '1e3');
  toggle(f.render, false);
  const tree = f.render();
  const fields = tree.filter((node) => node.type === 'Field');
  assert.equal(fields[0]!.props.editable, false);
  assert.equal(fields[1]!.props.editable, false);
  assert.equal(fields[0]!.props.value, '9007199254.740995');
  assert.equal(fields[1]!.props.value, '9007199254.740993');
  const review = button(tree, 'ownerPolicy.review');
  assert.equal(review.props.disabled, false);
  review.props.onPress();
  await settle();
  assert.deepEqual(f.f.prepares[0]!.slice(3), ['9007199254740995', '9007199254740993', false]);
  assert.equal(f.session.getSnapshot().phase, 'review');
  assert.equal(f.f.submissions.length, 0);
  button(f.render(), 'ownerPolicy.disable').props.onPress();
  await settle();
  assert.equal(f.f.submissions.length, 1);
  assert.equal(f.session.getSnapshot().phase, 'pending');
});

for (const [label, patch] of [
  ['another service', { allowed_profiles: ['f'.repeat(64)] }],
  ['another version', { version: 2 }],
  ['nonzero limit', { limit_units: '1' }],
  ['enabled', { enabled: true }],
  ['missing canonical policy', { canonical_policy_json: '' }],
  ['invalid digest', { policy_digest: 'not-a-digest' }],
] satisfies [string, Partial<ReturnType<typeof coreReview>>][])
  test(`first disable cannot confirm a Core review with ${label}`, async () => {
    const f = await account();
    f.port.prepare = async (owner, _source, _digest, limit, attempt, enabled) => ({
      ...coreReview(owner, limit, attempt, enabled),
      ...patch,
    });
    toggle(f.render, false);
    button(f.render(), 'ownerPolicy.review').props.onPress();
    await settle();
    assert.equal(f.session.getSnapshot().phase, 'editing');
    assert.equal(f.session.getSnapshot().error, 'prepare');
    assert.equal(f.f.submissions.length, 0);
    assert.equal(
      f.render().some((node) => node.props.label === 'ownerPolicy.disable'),
      false,
    );
  });

for (const [label, patch] of [
  ['missing digest', { current_policy_digest: undefined }],
  ['missing limit', { current_limit_units: undefined }],
  ['malformed digest', { current_policy_digest: 'not-a-digest' }],
  ['inconsistent null policy', { current_limit_units: '0' }],
  ['malformed image profile', { image_profile_digest: 'not-a-profile' }],
] as const)
  test(`incomplete account context never becomes a zero disable: ${label}`, async () => {
    const f = await account(context(patch));
    assert.equal(f.session.getSnapshot().context, null);
    assert.equal(
      f.render().some((node) => node.type === 'Switch'),
      false,
    );
    assert.equal(f.f.prepares.length, 0);
    assert.equal(f.f.submissions.length, 0);
  });

test('expired inquiry cannot reuse the old first-disable callback', async () => {
  const f = await account();
  toggle(f.render, false);
  const old = button(f.render(), 'ownerPolicy.review');
  f.port.options = async (_scope, source) => ({
    source,
    inquiry_digest: 'e'.repeat(64),
    state: 'expired',
    context: null,
  });
  await f.session.reconcile();
  old.props.onPress();
  await settle();
  assert.equal(f.session.getSnapshot().context, null);
  assert.equal(f.f.prepares.length, 0);
  assert.equal(f.f.submissions.length, 0);
});

test('first-disable lost acknowledgement resumes its exact receipt and fresh disabled context', async () => {
  const f = await account();
  toggle(f.render, false);
  button(f.render(), 'ownerPolicy.review').props.onPress();
  await settle();
  f.port.submit = async (_owner, value) => {
    f.f.submissions.push(value);
    throw Error('lost acknowledgement after enqueue');
  };
  f.port.find = async () => null;
  button(f.render(), 'ownerPolicy.disable').props.onPress();
  await settle();
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(has(f.render(), 'ownerPolicy.installed'), false);
  const queries = f.f.queries;
  f.f.receipt = { ...f.f.receipt, state: 'installed' };
  f.port.find = async () => f.f.receipt;
  f.f.context = context({
    current_policy_digest: policy,
    current_limit_units: '0',
    current_max_attempt_units: '0',
    current_enabled: false,
    current_expired: false,
    current_expires_at_ms: '200000',
    existing_profile_count: 1,
  });
  f.session.retire();
  f.session.activate(f.f.owner);
  await f.session.reconcile();
  await settle();
  const reopened = screen(f, 'account');
  assert.equal(f.session.getSnapshot().receipt?.source, f.f.receipt.source);
  assert.equal(f.session.getSnapshot().receipt?.policy_digest, policy);
  assert.equal(f.session.getSnapshot().effectiveness, 'disabled');
  assert.equal(f.f.queries, queries + 1);
  assert.equal(has(reopened(), 'ownerPolicy.disabled'), true);
  assert.equal(has(reopened(), 'ownerPolicy.installed'), false);
  assert.equal(f.f.prepares.length, 1);
  assert.equal(f.f.submissions.length, 1);
});
