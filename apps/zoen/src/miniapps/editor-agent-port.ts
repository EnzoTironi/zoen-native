import type { ClientStore } from '../core/store';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { homeSpacesFor } from '../home/access';
import type { MiniappDocumentController } from './document-controller';
import type { EditorAgentPort } from './editor-agent-session';

export function editorAgentScope(store: ClientStore, spaceId: string) {
  return homeSpacesFor(store.getSnapshot()).has(spaceId)
    ? accountScopeKey(accountScope(store))
    : '';
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
    send: async (text, head, mutation_id) => {
      const scope = check();
      const result = await store.request({
        op: 'request_app_edit',
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
