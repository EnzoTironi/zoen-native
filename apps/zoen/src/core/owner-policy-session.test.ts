import assert from 'node:assert/strict';
import { test } from 'node:test';
import { OwnerPolicySession, type PolicyPort, type PolicyScope } from '../owner-policy/session.ts';
import type { OwnerPolicyContext } from './generated/OwnerPolicyContext.ts';
import type { OwnerPolicyReview } from './generated/OwnerPolicyReview.ts';
import type { OwnerPolicyStatus } from './generated/OwnerPolicyStatus.ts';
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
async function settle() {
  for (let i = 0; i < 8; i++) await tick();
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const policy = 'c'.repeat(64),
  profile = 'a'.repeat(64);
const owner: PolicyScope = {
  identity: 'owner',
  device: 'device',
  relay: 'https://relay.test',
  epoch: 1,
  spaceId: 'private-dm',
  agentId: 'agent',
  requestId: 'image-request',
  expectedHash: 'request-hash',
};
function context(patch: Partial<OwnerPolicyContext> = {}): OwnerPolicyContext {
  return {
    options_digest: 'd'.repeat(64),
    currency: 'USD',
    scale: 6,
    year: 2026,
    month: 10,
    reservation_units: '100',
    current_limit_units: null,
    current_max_attempt_units: null,
    current_enabled: null,
    current_expired: null,
    current_expires_at_ms: null,
    current_policy_digest: null,
    existing_profile_count: 0,
    image_profile_digest: profile,
    image_size: '1024x1024',
    image_quality: 'low',
    observed_at_ms: '100000',
    ...patch,
  };
}
function review(
  scope: PolicyScope,
  limit: string,
  attempt: string,
  enabled: boolean,
): OwnerPolicyReview {
  return {
    scope: {
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
      space_id: scope.spaceId,
      agent_id: scope.agentId,
    },
    canonical_policy_json: '{"exact":"canonical policy bytes"}',
    policy_digest: policy,
    confirmation_digest: 'b'.repeat(64),
    currency: 'USD',
    scale: 6,
    year: 2026,
    month: 10,
    version: 1,
    limit_units: limit,
    max_attempt_units: attempt,
    expires_at_ms: '200000',
    enabled,
    allowed_profiles: [profile],
  };
}
async function fixture() {
  const f = {
    scope: { ...owner } as PolicyScope | null,
    context: context(),
    receipt: {
      source: 'original-policy',
      policy_digest: policy,
      state: 'pending',
    } as OwnerPolicyStatus,
    submits: 0,
    prepares: 0,
    queries: 0,
    finds: 0,
  };
  const options = (source: string) => ({
    source,
    inquiry_digest: 'e'.repeat(64),
    state: 'ready',
    context: f.context,
  });
  const port: PolicyPort = {
    scope: () => f.scope,
    inquire: async () => options(`query-${++f.queries}`),
    options: async (_scope, source) => options(source),
    findOptions: async () => null,
    prepare: async (scope, _source, _digest, limit, attempt, enabled) => {
      f.prepares++;
      return review(scope, limit, attempt, enabled);
    },
    submit: async () => {
      f.submits++;
      return f.receipt;
    },
    status: async () => f.receipt,
    find: async () => {
      f.finds++;
      return f.receipt;
    },
    changed: () => {},
  };
  const session = new OwnerPolicySession(port);
  session.activate(f.scope);
  await session.refresh();
  await settle();
  return { f, port, session };
}
async function ready(f: Awaited<ReturnType<typeof fixture>>) {
  f.session.edit(f.session.getSnapshot(), {
    limitInput: '9007199254740993',
    maxAttemptInput: '100',
  });
  await f.session.prepare();
  const confirmation = f.session.confirmation();
  assert.ok(confirmation);
  return confirmation;
}

test('exact-unit human review sends once and remains pending until matching actual receipt', async () => {
  const f = await fixture();
  assert.equal(f.session.getSnapshot().limitInput, '', 'no implicit cap');
  await f.session.prepare();
  assert.equal(f.f.prepares, 0);
  const confirmation = await ready(f);
  assert.equal(f.session.getSnapshot().review?.limit_units, '9007199254740993');
  await Promise.all([f.session.submit(confirmation), f.session.submit(confirmation)]);
  await settle();
  assert.equal(f.f.submits, 1);
  assert.equal(f.session.getSnapshot().phase, 'pending');
  assert.equal(f.session.getSnapshot().effectiveness, 'unknown');
  f.f.receipt = { ...f.f.receipt, state: 'installed' };
  f.f.context = context({
    current_policy_digest: policy,
    current_limit_units: '9007199254740993',
    current_max_attempt_units: '100',
    current_enabled: true,
    current_expired: false,
    current_expires_at_ms: '200000',
    existing_profile_count: 1,
  });
  await f.session.reconcile();
  await settle();
  assert.equal(f.session.getSnapshot().phase, 'installed');
  assert.equal(f.session.getSnapshot().effectiveness, 'current');
  assert.equal(f.f.queries, 2, 'one fresh Runtime inquiry after install');
  assert.equal(f.f.submits, 1);
});
test('editing during prepare discards old preview and old rendered edits', async () => {
  const f = await fixture(),
    gate = deferred<OwnerPolicyReview>();
  f.session.edit(f.session.getSnapshot(), { limitInput: '1000', maxAttemptInput: '100' });
  const rendered = f.session.getSnapshot();
  f.port.prepare = async () => gate.promise;
  const task = f.session.prepare();
  f.session.edit(f.session.getSnapshot(), { limitInput: '2000' });
  gate.resolve(review(owner, '1000', '100', true));
  await task;
  assert.equal(f.session.getSnapshot().phase, 'editing');
  assert.equal(f.session.getSnapshot().review, null);
  assert.equal(f.session.getSnapshot().limitInput, '2000');
  f.session.edit(rendered, { limitInput: '9999' });
  assert.equal(f.session.getSnapshot().limitInput, '2000');
});
test('canonical invalidation blocks old confirmation even if the quote is identical', async () => {
  const f = await fixture(),
    old = await ready(f);
  f.session.invalidate();
  assert.equal(f.session.confirmation(), null);
  await f.session.submit(old);
  assert.equal(f.f.submits, 0);
  await f.session.reconcile();
  await settle();
  const fresh = f.session.confirmation();
  assert.ok(fresh);
  await f.session.submit(old);
  assert.equal(f.f.submits, 0);
  await f.session.submit(fresh);
  await settle();
  assert.equal(f.f.submits, 1);
});
test('late options and previews cannot attach to another owner or reopened epoch', async () => {
  const f = await fixture(),
    gate = deferred<OwnerPolicyReview>();
  f.session.edit(f.session.getSnapshot(), { limitInput: '1000' });
  f.port.prepare = async () => gate.promise;
  const task = f.session.prepare();
  f.session.retire();
  f.f.scope = { ...owner, epoch: 2 };
  f.session.activate(f.f.scope);
  gate.resolve(review(owner, '1000', '100', true));
  await task;
  assert.equal(f.session.getSnapshot().review, null);
  assert.equal(f.session.getSnapshot().phase, 'loading');
  const query = deferred<Awaited<ReturnType<PolicyPort['inquire']>>>();
  f.port.inquire = async () => query.promise;
  const pending = f.session.refresh();
  f.session.retire();
  f.f.scope = { ...owner, identity: 'other-owner', epoch: 3 };
  f.session.activate(f.f.scope);
  query.resolve({
    source: 'old-query',
    inquiry_digest: 'e'.repeat(64),
    state: 'ready',
    context: context(),
  });
  await pending;
  assert.equal(f.session.getSnapshot().context, null);
  assert.equal(f.session.getSnapshot().optionsSource, null);
  assert.equal(f.f.submits, 0);
});
test('lost submit acknowledgement reads original without repeating the mutation', async () => {
  const f = await fixture(),
    confirmation = await ready(f);
  f.port.submit = async () => {
    f.f.submits++;
    throw Error('lost after durable enqueue');
  };
  await f.session.submit(confirmation);
  await settle();
  assert.equal(f.f.submits, 1);
  assert.ok(f.f.finds > 0);
  assert.equal(f.session.getSnapshot().receipt?.source, 'original-policy');
  assert.equal(f.session.getSnapshot().phase, 'pending');
  await f.session.submit(confirmation);
  assert.equal(f.f.submits, 1);
});
test('ambiguous submit with no original stays held and never restores Apply', async () => {
  const f = await fixture(),
    confirmation = await ready(f);
  f.port.submit = async () => {
    f.f.submits++;
    throw Error('unknown');
  };
  f.port.find = async () => null;
  await f.session.submit(confirmation);
  await settle();
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(f.session.confirmation(), null);
  f.session.edit(f.session.getSnapshot(), { limitInput: '9999' });
  await f.session.prepare();
  await f.session.submit(confirmation);
  assert.equal(f.f.submits, 1);
  assert.equal(f.session.getSnapshot().limitInput, '9007199254740993');
});
test('foreign digest receipt cannot turn an ambiguous result into success', async () => {
  const f = await fixture(),
    confirmation = await ready(f);
  f.f.receipt = { source: 'foreign', policy_digest: 'f'.repeat(64), state: 'installed' };
  await f.session.submit(confirmation);
  await settle();
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(f.session.getSnapshot().receipt, null);
  assert.equal(f.session.getSnapshot().effectiveness, 'unknown');
});
for (const [label, patch, result] of [
  ['superseded', { current_policy_digest: 'f'.repeat(64) }, 'superseded'],
  ['disabled', { current_enabled: false }, 'disabled'],
  ['expired', { current_expired: true }, 'expired'],
] as const)
  test(`historical installation is separate from ${label} current policy`, async () => {
    const f = await fixture(),
      confirmation = await ready(f);
    await f.session.submit(confirmation);
    await settle();
    f.f.receipt = { ...f.f.receipt, state: 'installed' };
    f.f.context = context({
      current_policy_digest: policy,
      current_limit_units: '1000',
      current_max_attempt_units: '100',
      current_enabled: true,
      current_expired: false,
      current_expires_at_ms: '200000',
      existing_profile_count: 1,
      ...patch,
    });
    await f.session.reconcile();
    await settle();
    assert.equal(f.session.getSnapshot().phase, 'installed');
    assert.equal(f.session.getSnapshot().effectiveness, result);
  });
test('focus retirement invalidates handlers but resumes one same-owner original', async () => {
  const f = await fixture(),
    confirmation = await ready(f);
  await f.session.submit(confirmation);
  await settle();
  f.session.retire();
  await f.session.submit(confirmation);
  assert.equal(f.f.submits, 1);
  f.session.activate(f.f.scope);
  await f.session.refresh();
  await settle();
  assert.equal(f.session.getSnapshot().receipt?.source, 'original-policy');
  assert.equal(f.session.getSnapshot().phase, 'pending');
  assert.equal(f.f.submits, 1);
  f.f.scope = { ...owner, device: 'other-device', epoch: 2 };
  f.session.activate(f.f.scope);
  assert.equal(f.session.getSnapshot().receipt, null);
  assert.equal(f.session.getSnapshot().review, null);
});

test('lost inquiry ACK recovers the original through reads without another query', async () => {
  const f = await fixture();
  let queries = 0;
  f.port.inquire = async () => {
    queries++;
    throw Error('lost inquiry acknowledgement');
  };
  f.port.findOptions = async () => ({
    source: 'persisted-new-query',
    inquiry_digest: 'e'.repeat(64),
    state: 'ready',
    context: context(),
  });
  await f.session.refresh();
  await settle();
  assert.equal(queries, 1);
  assert.equal(f.session.getSnapshot().optionsSource, 'persisted-new-query');
  assert.equal(f.session.getSnapshot().phase, 'editing');
});
test('missing inquiry after ambiguous ACK stays held and does not reuse previous quote', async () => {
  const f = await fixture();
  const previous = f.session.getSnapshot().optionsSource;
  let queries = 0;
  f.port.inquire = async () => {
    queries++;
    throw Error('unknown');
  };
  f.port.findOptions = async () => ({
    source: previous!,
    inquiry_digest: 'e'.repeat(64),
    state: 'ready',
    context: context(),
  });
  await f.session.refresh();
  await settle();
  assert.equal(queries, 1);
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(f.session.getSnapshot().context, null);
  assert.equal(f.session.confirmation(), null);
});
test('explicit beginEdit uses actual latest context and cannot release pending or held intent', async () => {
  const f = await fixture(),
    confirmation = await ready(f);
  await f.session.submit(confirmation);
  await settle();
  f.session.beginEdit();
  assert.equal(f.session.getSnapshot().phase, 'pending');
  f.f.receipt = { ...f.f.receipt, state: 'installed' };
  f.f.context = context({
    current_policy_digest: 'f'.repeat(64),
    current_limit_units: '2200',
    current_max_attempt_units: '200',
    current_enabled: false,
    current_expired: false,
    current_expires_at_ms: '200000',
    existing_profile_count: 1,
  });
  await f.session.reconcile();
  await settle();
  const old = f.session.getSnapshot();
  f.session.beginEdit(old);
  assert.equal(f.session.getSnapshot().phase, 'editing');
  assert.equal(f.session.getSnapshot().limitInput, '2200');
  assert.equal(f.session.getSnapshot().maxAttemptInput, '200');
  assert.equal(f.session.getSnapshot().enabled, false);
  assert.equal(f.session.getSnapshot().receipt, null);
  assert.equal(f.f.submits, 1);
});
