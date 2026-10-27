import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { ClientStore } from '../core/store.ts';
import type { AgentControlPort } from './agent-control-session.ts';

export function createAgentControlPort(store: ClientStore): AgentControlPort {
  const current = (scope: AccountScope) => {
    if (accountScopeKey(accountScope(store)) !== accountScopeKey(scope))
      throw new Error('The account session has changed.');
  };
  const owned = async (agent: string, scope: AccountScope) => {
    current(scope);
    const { persona } = await store.driver.request({ op: 'agent_profile', agent_id: agent });
    current(scope);
    if (
      persona.id !== agent ||
      persona.kind !== 'Agent' ||
      !persona.is_mine ||
      persona.owner_id !== scope.identity
    )
      throw new Error('The agent is not owned by this account.');
  };
  return {
    scope: () => accountScope(store),
    async read(agent, scope) {
      await owned(agent, scope);
      current(scope);
      const status = await store.driver.request({
        op: 'agent_control_read',
        agent_id: agent,
        expected_identity_id: scope.identity,
        expected_device_id: scope.device,
      });
      current(scope);
      if (
        !status ||
        !['Active', 'Paused', 'Revoked'].includes(status.status) ||
        !/^[1-9][0-9]*$/.test(status.revision) ||
        !['Observed', 'Unknown', 'Confirmed', 'Superseded'].includes(status.outcome) ||
        (status.pending !== null && !['Pause', 'Resume', 'Revoke'].includes(status.pending)) ||
        (status.pending !== null) !== (status.outcome === 'Unknown') ||
        (status.operation_id !== null && !/^[0-9a-f]{64}$/i.test(status.operation_id)) ||
        (status.pending !== null && status.operation_id === null)
      )
        throw new Error('The agent status is unavailable.');
      return status;
    },
    async retry(agent, action, scope, revision, operationId) {
      await owned(agent, scope);
      current(scope);
      await store.request({
        op: 'agent_control_retry',
        agent_id: agent,
        action,
        observed_revision: revision,
        observed_operation_id: operationId,
        expected_identity_id: scope.identity,
        expected_device_id: scope.device,
      });
      current(scope);
    },
    async control(agent, action, scope, revision) {
      await owned(agent, scope);
      current(scope);
      await store.request({
        op: 'agent_control_change',
        agent_id: agent,
        action,
        observed_revision: revision,
        expected_identity_id: scope.identity,
        expected_device_id: scope.device,
      });
      current(scope);
    },
  };
}
