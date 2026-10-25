import type { ClientState, ClientStore } from '../core/store';
import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import { homeSpacesFor } from '../home/access.ts';
import type { MiniappDocumentController } from './document-controller';
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
  return {
    scope: () => editorAgentScope(store, spaceId),
    id,
    flush: () => {
      check();
      return document.flush();
    },
    head: async () => {
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
      const value = await store.driver.request({
        op: 'app_binary_read',
        item_id: itemId,
        version: null,
        expected_sha256: null,
      });
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
    },
    requests: async () => {
      check();
      const value = await store.driver.request({ op: 'requests' });
      check();
      return value;
    },
    replies: async (sourceId) => {
      check();
      const page = await store.driver.request({
        op: 'timeline',
        space_id: spaceId,
        before: null,
        limit: 200,
      });
      check();
      return page.entries.filter((entry) => entry.reply_to?.id === sourceId);
    },
    send: async (text, head, mutation_id) => {
      const scope = check();
      const result = await store.request({
        op: 'request_app_edit',
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
