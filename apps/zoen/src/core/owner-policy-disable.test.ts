import assert from 'node:assert/strict';
import { test } from 'node:test';
import { validReview } from '../owner-policy/values.ts';
import { emptyPolicyState, type PolicyScope } from '../owner-policy/session.ts';
import type { OwnerPolicyContext } from './generated/OwnerPolicyContext.ts';
import type { OwnerPolicyReview } from './generated/OwnerPolicyReview.ts';

test('disabled review preserves sixteen approved services without approving a new configured image profile', () => {
  const profiles = Array.from({ length: 16 }, (_, n) => (n + 1).toString(16).padStart(64, '0'));
  const scope: PolicyScope = {
    identity: 'owner',
    device: 'phone',
    epoch: 1,
    relay: 'https://relay.test',
    purpose: 'account',
    spaceId: 'private-dm',
    agentId: 'agent',
  };
  const context: OwnerPolicyContext = {
    options_digest: 'a'.repeat(64),
    currency: 'USD',
    scale: 6,
    year: 2026,
    month: 10,
    reservation_units: '10',
    current_limit_units: '100',
    current_max_attempt_units: '10',
    current_enabled: true,
    current_expired: false,
    current_expires_at_ms: '200000',
    current_policy_digest: 'b'.repeat(64),
    existing_profile_count: 16,
    image_profile_digest: 'f'.repeat(64),
    image_size: '1024x1024',
    image_quality: 'low',
    observed_at_ms: '100000',
  };
  const state = {
    ...emptyPolicyState,
    context,
    limitInput: '100',
    maxAttemptInput: '10',
    enabled: false,
  };
  const review: OwnerPolicyReview = {
    scope: {
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
      space_id: scope.spaceId,
      agent_id: scope.agentId,
    },
    canonical_policy_json: '{"exact":"core-signed-policy"}',
    policy_digest: 'c'.repeat(64),
    confirmation_digest: 'd'.repeat(64),
    currency: 'USD',
    scale: 6,
    year: 2026,
    month: 10,
    version: 2,
    limit_units: '100',
    max_attempt_units: '10',
    expires_at_ms: '200000',
    enabled: false,
    allowed_profiles: profiles,
  };
  assert.equal(validReview(review, scope, state), true);
  assert.equal(
    validReview({ ...review, allowed_profiles: profiles.slice(1) }, scope, state),
    false,
  );
  assert.equal(
    validReview(
      { ...review, allowed_profiles: [...profiles, context.image_profile_digest] },
      scope,
      state,
    ),
    false,
  );
  assert.equal(
    validReview({ ...review, enabled: true }, scope, { ...state, enabled: true }),
    false,
  );
});
