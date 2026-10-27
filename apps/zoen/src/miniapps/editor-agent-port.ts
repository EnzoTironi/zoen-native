import type { ClientState, ClientStore } from '../core/store';
import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import { homeSpacesFor } from '../home/access.ts';
import type { MiniappDocumentController } from './document-controller';
import type { AppEditTargets } from '../core/generated/AppEditTargets';
import type { EditorAgentPort } from './editor-agent-session';

export function editorAgentScope(
  store: ClientStore,
  spaceId: string,
  observed: ClientState = store.getSnapshot(),
) {
  return homeSpacesFor(observed).has(spaceId) ? accountScopeKey(accountScope(store, observed)) : '';
}
export function editorAgentPort(
  store: ClientStore,
  itemId: string,
  spaceId: string,
  document: MiniappDocumentController,
  owner: string,
  id: () => string,
): EditorAgentPort {
  const check = () => {
    const scope = accountScope(store);
    if (!scope || !owner || editorAgentScope(store, spaceId) !== owner)
      throw Error('editor_agent_owner_changed');
    return scope;
  };
  const resourceUri =
    document.agentDocument?.kind === 'owned-scene' ? document.agentDocument.resourceUri : undefined;
  const head: EditorAgentPort['head'] = async () => {
    check();
    if (!document.observedHead) throw Error('editor_agent_head_unavailable');
    const observed = await document.observedHead();
    check();
    if (
      observed.itemId !== itemId ||
      !Number.isSafeInteger(observed.version) ||
      observed.version < 1 ||
      !/^[a-f0-9]{64}$/.test(observed.sha256)
    )
      throw Error('editor_agent_head_changed');
    const value = await store.driver.request(
      resourceUri !== undefined
        ? {
            op: 'app_document_read_pinned',
            item_id: itemId,
            expected_resource_uri: resourceUri,
          }
        : { op: 'app_binary_read', item_id: itemId, version: null, expected_sha256: null },
    );
    check();
    if (
      value.item_id !== itemId ||
      value.version !== value.current_version ||
      value.version !== observed.version ||
      value.sha256 !== observed.sha256 ||
      !Number.isSafeInteger(value.version) ||
      value.version < 1 ||
      !/^[a-f0-9]{64}$/.test(value.sha256)
    )
      throw Error('editor_agent_head_changed');
    return { version: value.version, sha256: value.sha256 };
  };
  return {
    scope: () => editorAgentScope(store, spaceId),
    id,
    flush: () => {
      check();
      return document.flush();
    },
    head,
    targets: async () => {
      const displayed = await head();
      const scope = check();
      const targets: AppEditTargets = await store.driver.request({
        op: 'app_edit_targets',
        input: {
          expected_identity_id: scope.identity,
          expected_device_id: scope.device,
          space_id: spaceId,
          item_id: itemId,
          expected_version: displayed.version,
          expected_sha256: displayed.sha256,
          expected_resource_uri: resourceUri,
        },
      });
      check();
      if (
        !/^[a-f0-9]{64}$/.test(targets.revision) ||
        new Set(targets.agents.map(({ agent }) => agent.id)).size !== targets.agents.length ||
        targets.agents.some(
          ({ agent }) =>
            !agent.is_mine || agent.kind !== 'Agent' || agent.owner_id !== scope.identity,
        )
      )
        throw Error('editor_agent_targets_invalid');
      return { head: displayed, targets };
    },
    requests: async () => {
      check();
      const value = await store.driver.request({ op: 'requests' });
      check();
      return value;
    },
    replies: async (sourceId) => {
      check();
      const entries = await store.driver.request({
        op: 'thread',
        space_id: spaceId,
        root_id: sourceId,
      });
      check();
      return entries;
    },
    send: async (text, head, mutation_id, target) => {
      const scope = check();
      if (target.head.version !== head.version || target.head.sha256 !== head.sha256)
        throw Error('editor_agent_target_changed');
      const result = await store.request({
        op: 'request_app_edit',
        target_agent_id: target.agent.id,
        expected_target_revision: target.revision,
        expected_resource_uri: resourceUri,
        share_current: true,
        expected_identity_id: scope.identity,
        expected_device_id: scope.device,
        space_id: spaceId,
        item_id: itemId,
        expected_version: head.version,
        expected_sha256: head.sha256,
        text,
        mutation_id,
      });
      check();
      return result;
    },
    decide: async (command) => {
      check();
      const value = await store.request(command);
      check();
      return value;
    },
  };
}
