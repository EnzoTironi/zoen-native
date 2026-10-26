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
      const status = await store.driver.request({ op: 'agent_status', agent_id: agent });
      current(scope);
      if (status !== 'Active' && status !== 'Paused' && status !== 'Revoked')
        throw new Error('The agent status is unavailable.');
      return status;
    },
    async control(agent, action, scope) {
      await owned(agent, scope);
      current(scope);
      await store.request({ op: 'control_agent', agent_id: agent, action });
      current(scope);
    },
  };
}
