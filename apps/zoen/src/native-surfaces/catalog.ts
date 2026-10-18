import type { AppIntentEntity } from 'expo-app-intents';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { surfaceId, validSurfaceOwner } from './snapshot.ts';
import type { SurfaceOwner } from './snapshot';

/** Explicitly selected, currently accessible records only; never publish chat previews or page bodies. */
export function itemEntityCatalog(
  items: ReadonlyArray<Pick<ItemDetail, 'id' | 'space_id' | 'title' | 'version'>>,
  owner: SurfaceOwner,
  selectedItemIds: ReadonlySet<string>,
  accessibleSpaceIds: ReadonlySet<string>,
): AppIntentEntity[] {
  if (!validSurfaceOwner(owner)) return [];
  const seen = new Set<string>();
  const entities: AppIntentEntity[] = [];
  for (const item of items) {
    if (entities.length >= 50) break;
    if (
      seen.has(item.id) ||
      !selectedItemIds.has(item.id) ||
      !accessibleSpaceIds.has(item.space_id) ||
      !surfaceId(item.id) ||
      !Number.isSafeInteger(item.version) ||
      item.version < 0
    )
      continue;
    const title = [...item.title]
      .filter((character) => {
        const code = character.charCodeAt(0);
        return code >= 32 && code !== 127;
      })
      .slice(0, 80)
      .join('')
      .trim();
    if (!title) continue;
    seen.add(item.id);
    entities.push({
      id: item.id,
      title,
      hideInSpotlight: true,
      metadata: {
        identityId: owner.identityId,
        deviceId: owner.deviceId,
        version: String(item.version),
      },
    });
  }
  return entities;
}
