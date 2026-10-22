import type { ClientStore } from '../core/store';
import type { AgentEnrollmentPreviewDto } from '../core/generated/AgentEnrollmentPreviewDto';
import {
  enrollmentScope,
  enrollmentScopeKey,
  type EnrollmentPort,
  type EnrollmentReview,
  type EnrollmentScope,
} from './session.ts';

function displayReview(value: AgentEnrollmentPreviewDto): EnrollmentReview {
  return {
    agent: value.agent_id,
    digest: value.offer_digest,
    name: value.name,
    handle: value.handle,
    persona: value.persona,
    rules: value.rules,
    pairing: value.pairing_check,
    expires: BigInt(value.expires_ms),
  };
}

export function createEnrollmentPort(
  store: Pick<ClientStore, 'driver' | 'request' | 'getSnapshot'>,
): EnrollmentPort {
  const current = (scope: EnrollmentScope) => {
    const state = store.getSnapshot();
    if (
      state.phase !== 'ready' ||
      state.erasePending ||
      enrollmentScopeKey(enrollmentScope(state.mode, state.snapshot?.account)) !==
        enrollmentScopeKey(scope)
    )
      throw new Error('The account session has changed.');
  };
  const expected = (scope: EnrollmentScope) => ({
    expected_identity_id: scope.owner,
    expected_device_id: scope.device,
  });
  return {
    async preview(code, scope) {
      current(scope);
      const value = await store.driver.request({
        op: 'agent_enrollment_preview',
        code,
        ...expected(scope),
      });
      current(scope);
      return displayReview(value);
    },
    async approve(code, review, scope) {
      current(scope);
      const value = await store.request({
        op: 'agent_enrollment_approve',
        code,
        observed_offer_digest: review.digest,
        confirmed_pairing_check: review.pairing,
        ...expected(scope),
      });
      current(scope);
      return {
        receipt: value.receipt_id,
        agent: value.agent_id,
        digest: value.offer_digest,
        outcome: value.outcome,
      };
    },
    async receipt(scope) {
      current(scope);
      // A successful read is also the compatibility probe. A missing command or
      // any failed probe keeps the entire input/approval UI closed.
      const value = await store.driver.request({
        op: 'agent_enrollment_status',
        ...expected(scope),
      });
      current(scope);
      if (!value) return null;
      if (
        value.preview.agent_id !== value.delivery.agent_id ||
        value.preview.offer_digest !== value.delivery.offer_digest ||
        !value.delivery.receipt_id
      )
        throw new Error('The connection receipt has changed.');
      return {
        review: displayReview(value.preview),
        receipt: value.delivery.receipt_id,
        outcome: value.delivery.outcome,
      };
    },
    async dismiss(receipt, review, scope) {
      current(scope);
      const value = await store.request({
        op: 'agent_enrollment_dismiss',
        receipt_id: receipt,
        observed_offer_digest: review.digest,
        ...expected(scope),
      });
      current(scope);
      return value;
    },
    async registration(review, scope) {
      current(scope);
      // Existing canonical discovery persists the public profile in the normal
      // identity projection, so it remains in Permissions after closing this flow.
      const found = await store.request({ op: 'find_people', query: review.handle });
      current(scope);
      if (
        !found.some(
          (person) =>
            person.id === review.agent &&
            person.handle === review.handle &&
            person.kind === 'Agent' &&
            person.owner_id === scope.owner &&
            person.is_mine,
        )
      )
        return null;
      const status = await store.driver.request({ op: 'agent_status', agent_id: review.agent });
      current(scope);
      return status;
    },
  };
}
