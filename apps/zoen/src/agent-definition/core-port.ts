import type { ClientStore } from '../core/store';
import { enrollmentScope, enrollmentScopeKey } from '../agent-enrollment/session.ts';
import type { DefinitionPort, DefinitionScope } from './session';

export function createDefinitionPort(
  store: Pick<ClientStore, 'driver' | 'request' | 'getSnapshot'>,
): DefinitionPort {
  const current = (scope: DefinitionScope) => {
    const state = store.getSnapshot();
    if (
      state.phase !== 'ready' ||
      state.erasePending ||
      enrollmentScopeKey(enrollmentScope(state.mode, state.snapshot?.account)) !==
        enrollmentScopeKey(scope)
    )
      throw Error('Account session changed');
  };
  const expected = (scope: DefinitionScope) => ({
    agent_id: scope.agent,
    expected_identity_id: scope.owner,
    expected_device_id: scope.device,
  });
  const intent = (
    scope: DefinitionScope,
    baseline: Parameters<DefinitionPort['preview']>[1],
    persona: string,
    rules: string[],
  ) => ({
    ...expected(scope),
    observed_version: baseline.version,
    observed_definition_sha256: baseline.definition_sha256,
    persona,
    rules,
  });
  return {
    async read(scope) {
      current(scope);
      const value = await store.driver.request({ op: 'agent_definition_read', ...expected(scope) });
      current(scope);
      return value;
    },
    async preview(scope, baseline, persona, rules) {
      current(scope);
      const value = await store.driver.request({
        op: 'agent_definition_preview',
        ...intent(scope, baseline, persona, rules),
      });
      current(scope);
      return value;
    },
    async update(scope, review) {
      current(scope);
      const value = await store.request({
        op: 'agent_definition_update',
        ...intent(scope, review.current, review.proposed.persona, [...review.proposed.rules]),
        observed_preview_digest: review.preview_digest,
      });
      current(scope);
      return value;
    },
    async reconcile(scope) {
      current(scope);
      const value = await store.driver.request({
        op: 'agent_definition_reconcile',
        ...expected(scope),
      });
      current(scope);
      return value;
    },
    async dismiss(scope, receipt) {
      current(scope);
      const value = await store.request({
        op: 'agent_definition_dismiss',
        ...expected(scope),
        receipt_id: receipt.receipt_id,
        observed_preview_digest: receipt.review.preview_digest,
      });
      current(scope);
      return value;
    },
  };
}
