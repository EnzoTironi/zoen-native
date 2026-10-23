import type { ClientStore } from '../core/store';
import type { PageDto } from '../core/generated/PageDto';
import { homeSpacesFor } from '../home/access.ts';
import { t } from '../i18n/index.ts';
import { documentScope } from './document-authority.ts';

type ExportStore = Pick<ClientStore, 'getSnapshot' | 'sessionEpoch' | 'driver'>;
export type PageExportBinding = Readonly<{
  scope: string;
  spaceId: string;
  itemId: string;
  version: number;
  contentHash: string;
}>;
export function pageExportBinding(store: ExportStore, page: PageDto): PageExportBinding | null {
  const state = store.getSnapshot();
  const scope = documentScope(state, store.sessionEpoch);
  return scope && page.ready && page.version > 0 && homeSpacesFor(state).has(page.space_id)
    ? {
        scope,
        spaceId: page.space_id,
        itemId: page.item_id,
        version: page.version,
        contentHash: page.content_hash,
      }
    : null;
}
export function currentPageExport(store: ExportStore, binding: PageExportBinding): boolean {
  const state = store.getSnapshot();
  return (
    documentScope(state, store.sessionEpoch) === binding.scope &&
    homeSpacesFor(state).has(binding.spaceId)
  );
}
/** This read never commits a draft. Recheck identity and membership before sharing. */
export async function exportObservedPage(
  store: ExportStore,
  binding: PageExportBinding,
  active: () => boolean,
  share: (title: string, markdown: string, isCurrent: () => boolean) => Promise<void>,
): Promise<boolean> {
  const current = () => active() && currentPageExport(store, binding);
  if (!current()) return false;
  const result = await store.driver.request({
    op: 'page_export_version',
    space_id: binding.spaceId,
    item_id: binding.itemId,
    version: binding.version,
    observed_hash: binding.contentHash,
  });
  if (!current()) return false;
  if (
    result.item_id !== binding.itemId ||
    result.space_id !== binding.spaceId ||
    result.version !== binding.version ||
    result.content_hash !== binding.contentHash ||
    typeof result.markdown !== 'string' ||
    typeof result.title !== 'string'
  )
    throw new Error(t('documents.thisVersionCouldNotBeLoaded'));
  await share(`${result.title} v${result.version}`, result.markdown, current);
  return current();
}
