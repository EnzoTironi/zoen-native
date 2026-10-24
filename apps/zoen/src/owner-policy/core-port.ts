import type { ClientStore } from '../core/store';
import type { OwnerPolicyScope } from '../core/generated/OwnerPolicyScope';
import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import { policyScopeKey, type PolicyPort, type PolicyScope, type PolicyTarget } from './session.ts';

export type PolicyUnavailable =
  | 'account-required'
  | 'private-conversation-required'
  | 'request-changed'
  | null;
export function policyAvailability(
  store: ClientStore,
  target: PolicyTarget,
): { scope: PolicyScope | null; reason: PolicyUnavailable } {
  const owner = accountScope(store),
    snapshot = store.getSnapshot().snapshot;
  if (!owner || !snapshot) return { scope: null, reason: 'account-required' };
  const space = snapshot.conversations.find((value) => value.id === target.spaceId);
  if (
    !space ||
    space.kind !== 'Direct' ||
    space.privacy !== 'EndToEnd' ||
    space.members.length !== 2 ||
    space.counterpart?.id !== target.agentId ||
    space.counterpart.kind !== 'Agent' ||
    space.counterpart.owner_id !== owner.identity ||
    !space.counterpart.is_mine ||
    !space.members.some((value) => value.id === owner.identity)
  )
    return { scope: null, reason: 'private-conversation-required' };
  if (target.purpose === 'account') {
    return target.requestId !== undefined || target.expectedHash !== undefined
      ? { scope: null, reason: 'request-changed' }
      : { scope: { ...owner, ...target }, reason: null };
  }
  const request = snapshot.requests.find((value) => value.id === target.requestId);
  if (
    !request ||
    request.space_id !== target.spaceId ||
    request.agent.id !== target.agentId ||
    request.status !== 'Pending' ||
    request.tool_approval?.content_hash !== target.expectedHash ||
    !request.tool_approval.image
  )
    return { scope: null, reason: 'request-changed' };
  return { scope: { ...owner, ...target }, reason: null };
}
const expected = (scope: PolicyScope): OwnerPolicyScope => ({
  expected_identity_id: scope.identity,
  expected_device_id: scope.device,
  space_id: scope.spaceId,
  agent_id: scope.agentId,
});
export function createOwnerPolicyPort(store: ClientStore, target: PolicyTarget): PolicyPort {
  const scope = () => policyAvailability(store, target).scope;
  const current = (observed: PolicyScope) => {
    if (
      policyScopeKey(scope()) !== policyScopeKey(observed) ||
      accountScopeKey(accountScope(store)) !== accountScopeKey(observed)
    )
      throw Error('Owner policy session changed');
  };
  return {
    scope,
    async inquire(observed) {
      current(observed);
      const value = await store.driver.request(
        observed.purpose === 'account'
          ? {
              op: 'request_account_policy_options',
              scope: expected(observed),
            }
          : {
              op: 'request_owner_policy_options',
              scope: expected(observed),
              request_id: observed.requestId,
              expected_hash: observed.expectedHash,
            },
      );
      current(observed);
      return value;
    },
    async findOptions(observed) {
      current(observed);
      const value = await store.driver.request(
        observed.purpose === 'account'
          ? {
              op: 'find_account_policy_options',
              scope: expected(observed),
            }
          : {
              op: 'find_owner_policy_options',
              scope: expected(observed),
              request_id: observed.requestId,
              expected_hash: observed.expectedHash,
            },
      );
      current(observed);
      return value;
    },
    async options(observed, source) {
      current(observed);
      const value = await store.driver.request({
        op: 'owner_policy_options',
        scope: expected(observed),
        source,
      });
      current(observed);
      return value;
    },
    async prepare(observed, source, options_digest, limit_units, max_attempt_units, enabled) {
      current(observed);
      const value = await store.driver.request({
        op: 'prepare_owner_policy',
        scope: expected(observed),
        source,
        options_digest,
        limit_units,
        max_attempt_units,
        enabled,
      });
      current(observed);
      return value;
    },
    async submit(observed, review) {
      current(observed);
      const value = await store.driver.request({
        op: 'submit_owner_policy',
        scope: expected(observed),
        policy_json: review.canonical_policy_json,
        confirmation_digest: review.confirmation_digest,
      });
      current(observed);
      return value;
    },
    async status(observed, source) {
      current(observed);
      const value = await store.driver.request({
        op: 'owner_policy_status',
        scope: expected(observed),
        source,
      });
      current(observed);
      return value;
    },
    async find(observed, policy_digest) {
      current(observed);
      const value = await store.driver.request({
        op: 'find_owner_policy_submission',
        scope: expected(observed),
        policy_digest,
      });
      current(observed);
      return value;
    },
    changed(observed) {
      if (policyScopeKey(scope()) === policyScopeKey(observed))
        void store.refresh([observed.spaceId]);
    },
  };
}
