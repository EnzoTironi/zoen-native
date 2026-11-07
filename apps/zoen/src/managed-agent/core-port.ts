import type { ClientStore } from '../core/store.ts';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import { createEnrollmentPort } from '../agent-enrollment/core-port.ts';
import type { ManagedAgentPort } from './session.ts';

export function managedAgentPort(store: ClientStore): ManagedAgentPort {
  const current = (scope: AccountScope) => {
    if (accountScopeKey(scope) !== accountScopeKey(accountScope(store)))
      throw Error('The account session has changed.');
  };
  const expected = (scope: AccountScope) => ({
    expected_identity_id: scope.identity,
    expected_device_id: scope.device,
  });
  const registration = createEnrollmentPort(store);
  return {
    async status(scope) {
      current(scope);
      const value = await store.driver.request({ op: 'managed_agent_status', ...expected(scope) });
      current(scope);
      return value;
    },
    async start(name, scope) {
      current(scope);
      const value = await store.driver.request({
        op: 'managed_agent_start',
        name,
        ...expected(scope),
      });
      current(scope);
      return value;
    },
    async approve(view, scope) {
      current(scope);
      if (!view.preview) throw Error('The offer has changed.');
      const value = await store.driver.request({
        op: 'managed_agent_approve',
        operation_id: view.operation_id,
        observed_offer_digest: view.preview.offer_digest,
        ...expected(scope),
      });
      current(scope);
      return value;
    },
    async verify(view, scope) {
      current(scope);
      if (view.state !== 'registered' || !view.agent_id) throw Error('Agent not installed.');
      const status = await registration.registration(
        { agent: view.agent_id, handle: view.handle },
        { owner: scope.identity, device: scope.device, relay: scope.relay },
      );
      current(scope);
      if (status !== 'Active') throw Error('Agent unavailable.');
    },
    async chat(view, scope) {
      current(scope);
      if (!view.agent_id) throw Error('Agent not installed.');
      const value = await store.driver.request({
        op: 'managed_agent_chat',
        operation_id: view.operation_id,
        agent_id: view.agent_id,
        ...expected(scope),
      });
      current(scope);
      await store.refresh([value]);
      current(scope);
      return value;
    },
    async discard(view, scope) {
      current(scope);
      const done = await store.driver.request({
        op: 'managed_agent_discard',
        operation_id: view.operation_id,
        ...expected(scope),
      });
      current(scope);
      if (!done) throw Error('Offer was not retired.');
    },
    async complete(view, scope) {
      current(scope);
      if (!view.agent_id) throw Error('Agent not installed.');
      const completed = await store.driver.request({
        op: 'managed_agent_complete',
        operation_id: view.operation_id,
        agent_id: view.agent_id,
        ...expected(scope),
      });
      current(scope);
      if (!completed) throw Error('Installation completion not confirmed.');
    },
  };
}
