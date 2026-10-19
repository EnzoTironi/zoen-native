import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';
import { t } from '../i18n/index.ts';
import { surfaceItemURL } from './links.ts';
import type { WidgetSelection } from './preferences';
import { surfaceId, validSurfaceOwner, widgetProjection } from './snapshot.ts';
import type { NativeWidgetContent, SnapshotItem, SurfaceOwner } from './snapshot';

export const MAX_WIDGET_SELECTIONS = 20;
export type WidgetCatalogEntry = Readonly<{
  selectionId: string;
  configurationTitle: string;
  content: NativeWidgetContent;
}>;
export type NativeWidgetCatalog = Readonly<{
  entries: readonly WidgetCatalogEntry[];
  placeholder: NativeWidgetContent;
}>;

export function validWidgetSelectionId(value: unknown): value is string {
  return typeof value === 'string' && /^w_[a-f0-9]{64}$/.test(value);
}

/** Opaque configuration values bind an owner/device/item, never an array index or version. */
export function widgetSelectionId(owner: SurfaceOwner, itemId: string): string {
  if (!owner || !validSurfaceOwner(owner) || !surfaceId(itemId))
    throw new Error('Invalid widget selection');
  const scope = JSON.stringify([
    'zoen.widget.selection.v1',
    owner.identityId,
    owner.deviceId,
    itemId,
  ]);
  return `w_${bytesToHex(sha256(utf8ToBytes(scope)))}`;
}

/** Keep private configuration names aligned with the device's preference order. */
export function privateWidgetTitle(ordinal: number): string {
  if (!Number.isInteger(ordinal) || ordinal < 1 || ordinal > MAX_WIDGET_SELECTIONS)
    throw new Error('Invalid widget ordinal');
  return t('nativeSurfaces.privateWidgetTitle', { number: ordinal });
}

/** Only explicit, accessible compact projections enter the extension. Missing IDs never fall back. */
export function widgetCatalog(
  items: readonly SnapshotItem[],
  owner: SurfaceOwner,
  selections: readonly WidgetSelection[],
  accessibleSpaceIds: ReadonlySet<string>,
): NativeWidgetCatalog | null {
  if (
    !owner ||
    !validSurfaceOwner(owner) ||
    !Array.isArray(selections) ||
    selections.length > MAX_WIDGET_SELECTIONS ||
    selections.some(
      (selection) =>
        !selection || !surfaceId(selection.itemId) || typeof selection.showContent !== 'boolean',
    )
  )
    return null;
  const entries: WidgetCatalogEntry[] = [];
  const byId = new Map(items.map((item) => [item.id, item]));
  const seen = new Set<string>();
  for (const [index, selection] of selections.entries()) {
    if (seen.has(selection.itemId)) continue;
    seen.add(selection.itemId);
    const item = byId.get(selection.itemId);
    if (!item || !accessibleSpaceIds.has(item.space_id)) continue;
    const content = widgetProjection(item, owner, selection.showContent);
    if (!content) continue;
    entries.push(
      Object.freeze({
        selectionId: widgetSelectionId(owner, item.id),
        configurationTitle: selection.showContent ? content.title : privateWidgetTitle(index + 1),
        content: Object.freeze({
          ...content,
          deepLink: surfaceItemURL(owner, item.id, item.version),
        }),
      }),
    );
  }
  return Object.freeze({
    entries: Object.freeze(entries),
    placeholder: Object.freeze({
      redacted: true,
      title: 'Zoen',
      detail: t('nativeSurfaces.chooseAMiniappInZoen'),
      bars: Object.freeze([]),
      rows: Object.freeze([]),
      accentHex: '#5B8DEF',
      symbol: 'lock.fill',
    }),
  });
}
