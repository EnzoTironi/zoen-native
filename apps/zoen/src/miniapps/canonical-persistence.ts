import type { ClientStore } from '../core/store';
import type { CanonicalSceneSave } from './canonical-scene';
import { isOwnedAppResource, type CanonicalResourcePin } from './canonical-resource';

type Copy = Readonly<{ item_id: string; scene_json: string; mutation_id: string }>;
/** One captured renderer pin across reads, checkpoints and explicit Save Copy. */
export function canonicalPersistence(
  store: ClientStore,
  pin: CanonicalResourcePin,
  check: () => void,
) {
  const confirm = async <T>(work: () => Promise<T>) => {
    check();
    const result = await work();
    check();
    return result;
  };
  const owned = isOwnedAppResource(pin.resourceUri);
  return {
    read: () =>
      confirm(() =>
        store.driver.request(
          owned
            ? {
                op: 'app_document_read_pinned',
                item_id: pin.itemId,
                expected_resource_uri: pin.resourceUri,
              }
            : { op: 'app_document_read', item_id: pin.itemId },
        ),
      ),
    save: (input: CanonicalSceneSave) =>
      confirm(() =>
        store.request(
          owned
            ? { op: 'app_document_save_pinned', expected_resource_uri: pin.resourceUri, ...input }
            : { op: 'app_document_save', ...input },
        ),
      ),
    copy: (input: Copy) =>
      confirm(() =>
        store.request(
          owned
            ? { op: 'app_document_copy_pinned', expected_resource_uri: pin.resourceUri, ...input }
            : { op: 'app_document_copy', ...input },
        ),
      ),
  };
}
