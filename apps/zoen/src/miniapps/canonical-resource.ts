import type { Command, CommandResult, ItemDetail } from '../core/types';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { t } from '../i18n/index.ts';
import { readVerifiedMiniappResource } from './resource-load.ts';

export type CanonicalResourcePin = Readonly<{
  itemId: string;
  resourceUri: string;
  appId: string;
}>;
export type AppResourceRequest = <C extends Command>(command: C) => Promise<CommandResult<C>>;

/** Core-owned Item metadata, captured once for this focused editor lifetime. */
export function canonicalResourcePin(item: ItemDetail, spec: AppSpecDto): CanonicalResourcePin {
  return Object.freeze({
    itemId: item.id,
    resourceUri: item.app?.resource_uri ?? spec.resource_uri,
    appId: item.app?.app_id ?? spec.id,
  });
}

/** Routing only: Core authorizes the Item, exact URI and bytes on every operation. */
export function isOwnedAppResource(uri: string): boolean {
  return uri.startsWith('ui://zoen/package/');
}

/** Both reads bind to the same immutable pin. A concurrent release update refuses the pair. */
export async function readCanonicalResource(
  pin: CanonicalResourcePin,
  request: AppResourceRequest,
  check: () => void,
) {
  check();
  const owned = isOwnedAppResource(pin.resourceUri);
  if (!owned && pin.resourceUri !== 'ui://zoen/excalidraw.html')
    throw new Error(t('miniapps.thisResourceDoesNotBelongToTheOpen'));
  const [verified, doc] = await Promise.all([
    readVerifiedMiniappResource(
      () =>
        owned
          ? request({
              op: 'app_document_resource',
              item_id: pin.itemId,
              expected_resource_uri: pin.resourceUri,
            })
          : request({ op: 'app_resource', uri: pin.resourceUri }),
      pin.appId,
      pin.resourceUri,
      check,
    ),
    owned
      ? request({
          op: 'app_document_read_pinned',
          item_id: pin.itemId,
          expected_resource_uri: pin.resourceUri,
        })
      : request({ op: 'app_document_read', item_id: pin.itemId }),
  ]);
  check();
  if (verified.manifest.allowedDomains.length || verified.manifest.capabilities.length)
    throw new Error(t('miniapps.theMiniappManifestDoesNotMatchThisResource'));
  if (doc.item_id !== pin.itemId) throw new Error(t('drawing.unavailable'));
  return { verified, doc };
}
