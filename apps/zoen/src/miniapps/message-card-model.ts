import type { ItemCard } from '../core/generated/ItemCard';
import type { ItemDetail } from '../core/generated/ItemDetail';

export type MiniappMessageProjection = Pick<
  ItemDetail,
  'id' | 'space_id' | 'version' | 'title' | 'app'
>;

/** A signed message already contains a real app projection. A live item may only
 * supersede it when identity, scope, app and version all agree. No per-row reads. */
export function miniappMessageProjection(
  card: ItemCard,
  item?: ItemDetail | null,
  spaceId?: string,
): MiniappMessageProjection | null {
  if (card.kind_id !== 'app' || !card.app) return null;
  const scope = spaceId ?? (item?.id === card.item_id ? item.space_id : undefined);
  if (!scope) return null;
  if (
    item?.id === card.item_id &&
    item.kind_id === 'app' &&
    item.space_id === scope &&
    item.app?.app_id === card.app.app_id &&
    item.version >= card.version
  ) {
    return {
      id: item.id,
      space_id: item.space_id,
      version: item.version,
      title: item.title,
      app: item.app,
    };
  }
  return {
    id: card.item_id,
    space_id: scope,
    version: card.version,
    title: card.title,
    app: card.app,
  };
}
