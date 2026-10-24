import type { OwnerPolicyContext } from '../core/generated/OwnerPolicyContext.ts';
import type { OwnerPolicyReview } from '../core/generated/OwnerPolicyReview.ts';
import type { OwnerPolicyStatus } from '../core/generated/OwnerPolicyStatus.ts';
import type { PolicyScope, PolicyState } from './session.ts';
export const digest = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
export const units = (value: unknown): value is string =>
  typeof value === 'string' &&
  /^(0|[1-9][0-9]{0,18})$/.test(value) &&
  BigInt(value) <= 9223372036854775807n;
const time = (value: unknown): value is string => units(value) && value !== '0';
export const frozenContext = (value: OwnerPolicyContext) => Object.freeze({ ...value });
export function frozenReview(value: OwnerPolicyReview): OwnerPolicyReview {
  const copy = {
    ...value,
    scope: { ...value.scope },
    allowed_profiles: [...value.allowed_profiles],
  };
  Object.freeze(copy.scope);
  Object.freeze(copy.allowed_profiles);
  return Object.freeze(copy);
}
export function effectiveness(
  receipt: OwnerPolicyStatus | null,
  context: OwnerPolicyContext | null,
): PolicyState['effectiveness'] {
  if (receipt?.state !== 'installed' || !context?.current_policy_digest) return 'unknown';
  if (context.current_policy_digest !== receipt.policy_digest) return 'superseded';
  if (context.current_expired) return 'expired';
  if (context.current_enabled === false) return 'disabled';
  return context.current_enabled === true ? 'current' : 'unknown';
}

export function validContext(value: OwnerPolicyContext) {
  return (
    digest(value.options_digest) &&
    value.currency === 'USD' &&
    value.scale === 6 &&
    Number.isInteger(value.year) &&
    value.year >= 2000 &&
    value.year <= 2200 &&
    Number.isInteger(value.month) &&
    value.month >= 1 &&
    value.month <= 12 &&
    units(value.reservation_units) &&
    time(value.observed_at_ms) &&
    digest(value.image_profile_digest) &&
    ['1024x1024', '1536x1024', '1024x1536'].includes(value.image_size) &&
    ['low', 'medium', 'high'].includes(value.image_quality) &&
    Number.isInteger(value.existing_profile_count) &&
    value.existing_profile_count >= 0 &&
    value.existing_profile_count <= 16 &&
    (value.current_policy_digest === null
      ? value.current_limit_units === null &&
        value.current_max_attempt_units === null &&
        value.current_enabled === null &&
        value.current_expired === null &&
        value.current_expires_at_ms === null &&
        value.existing_profile_count === 0
      : digest(value.current_policy_digest) &&
        units(value.current_limit_units) &&
        units(value.current_max_attempt_units) &&
        time(value.current_expires_at_ms) &&
        typeof value.current_enabled === 'boolean' &&
        typeof value.current_expired === 'boolean')
  );
}
export function validReview(value: OwnerPolicyReview, scope: PolicyScope, state: PolicyState) {
  const context = state.context;
  return (
    !!context &&
    value.scope.expected_identity_id === scope.identity &&
    value.scope.expected_device_id === scope.device &&
    value.scope.space_id === scope.spaceId &&
    value.scope.agent_id === scope.agentId &&
    digest(value.policy_digest) &&
    digest(value.confirmation_digest) &&
    typeof value.canonical_policy_json === 'string' &&
    value.canonical_policy_json.length > 0 &&
    value.canonical_policy_json.length <= 4096 &&
    value.currency === context.currency &&
    value.scale === context.scale &&
    value.year === context.year &&
    value.month === context.month &&
    value.limit_units === state.limitInput &&
    value.max_attempt_units === state.maxAttemptInput &&
    value.enabled === state.enabled &&
    time(value.expires_at_ms) &&
    Number.isSafeInteger(value.version) &&
    value.version > 0 &&
    value.version <= 4294967295 &&
    Array.isArray(value.allowed_profiles) &&
    value.allowed_profiles.length > 0 &&
    value.allowed_profiles.length <= 16 &&
    value.allowed_profiles.every(digest) &&
    value.allowed_profiles.includes(context.image_profile_digest) &&
    value.allowed_profiles.every(
      (profile, index) => index === 0 || value.allowed_profiles[index - 1] < profile,
    )
  );
}
