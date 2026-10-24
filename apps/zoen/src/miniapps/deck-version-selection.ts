import type { ClientStore } from '../core/store';
import type { ItemDetail } from '../core/types';
import { homeSpacesFor } from '../home/access.ts';
import { documentScope } from '../documents/document-authority.ts';
import { binaryEditorFormat, type BinaryEditorId } from './binary-formats.ts';
import type { DeckcraftExportHead } from './deckcraft-export';

type Store = Pick<ClientStore, 'getSnapshot' | 'sessionEpoch' | 'driver'>;
export type DeckVersionBinding = Readonly<{
  scope: string;
  editor: BinaryEditorId;
  spaceId: string;
  itemId: string;
  version: number;
}>;
export function deckVersionBinding(store: Store, item: ItemDetail): DeckVersionBinding | null {
  const state = store.getSnapshot();
  const scope = documentScope(state, store.sessionEpoch);
  return scope &&
    (item.app?.app_id === 'deckcraft' || item.app?.app_id === 'gridcraft') &&
    Number.isSafeInteger(item.version) &&
    item.version > 0 &&
    homeSpacesFor(state).has(item.space_id)
    ? {
        scope,
        editor: item.app.app_id,
        spaceId: item.space_id,
        itemId: item.id,
        version: item.version,
      }
    : null;
}
export function currentDeckVersion(store: Store, binding: DeckVersionBinding) {
  const state = store.getSnapshot();
  return (
    documentScope(state, store.sessionEpoch) === binding.scope &&
    homeSpacesFor(state).has(binding.spaceId)
  );
}
/** Resolve a historical hash through the authorized Core read, never from display metadata. */
export async function observeDeckVersion(
  store: Store,
  binding: DeckVersionBinding,
  active: () => boolean,
): Promise<DeckcraftExportHead | null> {
  const current = () => active() && currentDeckVersion(store, binding);
  if (!current()) return null;
  const source = await store.driver.request({
    op: 'app_binary_read',
    item_id: binding.itemId,
    version: binding.version,
    expected_sha256: null,
  });
  if (!current()) return null;
  if (
    source.item_id !== binding.itemId ||
    source.kind !== binding.editor ||
    source.version !== binding.version ||
    source.current_version < source.version ||
    source.engine_revision !== binaryEditorFormat(binding.editor).engineRevision ||
    !/^[a-f0-9]{64}$/.test(source.sha256)
  )
    throw Error('The selected document version could not be loaded.');
  return { version: String(source.version), sha256: source.sha256 };
}
export function selectedDeckHead(version: unknown, sha256: unknown): DeckcraftExportHead | null {
  if (
    typeof version !== 'string' ||
    !/^[1-9][0-9]{0,9}$/.test(version) ||
    Number(version) > 0xffffffff ||
    typeof sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(sha256)
  )
    return null;
  return { version, sha256 };
}
