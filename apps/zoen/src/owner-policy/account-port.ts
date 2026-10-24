import type { ClientStore } from '../core/store.ts';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import { policyAvailability } from './core-port.ts';
import type { PolicyTarget } from './session.ts';

/** Core repeats owned-agent validation inside the scoped command. */
export function ownedBudgetAgents(store: ClientStore) {
  const owner = accountScope(store);
  return owner
    ? (store.getSnapshot().snapshot?.agents ?? []).filter(
        ({ persona }) =>
          persona.kind === 'Agent' && persona.is_mine && persona.owner_id === owner.identity,
      )
    : [];
}
export async function openAccountBudget(
  store: ClientStore,
  owner: AccountScope,
  agentId: string,
): Promise<PolicyTarget> {
  const current = () => {
    if (
      accountScopeKey(owner) !== accountScopeKey(accountScope(store)) ||
      !ownedBudgetAgents(store).some(({ persona }) => persona.id === agentId)
    )
      throw Error('Account budget owner changed');
  };
  current();
  const spaceId = await store.driver.request({
    op: 'start_owner_policy_direct',
    expected_identity_id: owner.identity,
    expected_device_id: owner.device,
    agent_id: agentId,
  });
  current();
  await store.refresh([spaceId]);
  current();
  const target: PolicyTarget = { purpose: 'account', agentId, spaceId };
  if (!policyAvailability(store, target).scope) throw Error('Private budget context unavailable');
  return target;
}
