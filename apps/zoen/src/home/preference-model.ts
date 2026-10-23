import type { ItemDetail } from '../core/generated/ItemDetail';

export type HomeOwner = Readonly<{
  mode: 'account' | 'demo';
  identityId: string;
  deviceId: string;
}>;
export type HomeSource = {
  owner(): HomeOwner | null;
  spaces(): ReadonlySet<string>;
  items(): Promise<ItemDetail[]>;
  /** Volatile session/config revision; never part of the persisted preference key. */
  revision?(): string | number;
};
export type HomePreferencesPort = {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
};
export type HomeFailureKind = 'load' | 'save' | 'accountChanged';
export class HomeFailure extends Error {
  readonly kind: HomeFailureKind;
  constructor(kind: HomeFailureKind, cause?: unknown) {
    super(kind, { cause });
    this.kind = kind;
  }
}
export type HomePreference = Readonly<{
  pinnedIds: readonly string[];
  orderedIds: readonly string[];
  hiddenIds: readonly string[];
}>;
export type HomeState = HomePreference &
  Readonly<{
    owner: HomeOwner | null;
    items: ItemDetail[];
    editing: boolean;
    editSaving: boolean;
    loading: boolean;
    error: HomeFailure | null;
  }>;

export const itemId = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
export const emptyState = (): HomeState => ({
  owner: null,
  items: [],
  pinnedIds: [],
  orderedIds: [],
  hiddenIds: [],
  editing: false,
  editSaving: false,
  loading: false,
  error: null,
});
export function homeOwnerKey(owner: HomeOwner | null): string | null {
  return owner
    ? `home-pins:1:${JSON.stringify([owner.mode, owner.identityId, owner.deviceId])}`
    : null;
}
export const validIds = (value: unknown): value is string[] =>
  Array.isArray(value) && value.length <= 100 && value.every(itemId);
export const maxPreferenceLength = 49_152;
const emptyPreference = (): HomePreference => ({ pinnedIds: [], orderedIds: [], hiddenIds: [] });

/** Same per-device key as legacy favorites; no account content or cross-device order. */
export function parseHomePreference(value: string | null): HomePreference {
  if (!value || value.length > maxPreferenceLength) return emptyPreference();
  try {
    const decoded: unknown = JSON.parse(value);
    // Old favorite arrays and v2 orders migrate in memory; reads never rewrite them.
    if (validIds(decoded))
      return { pinnedIds: [...new Set(decoded)], orderedIds: [], hiddenIds: [] };
    if (!decoded || typeof decoded !== 'object' || Array.isArray(decoded)) return emptyPreference();
    const fields = Object.keys(decoded);
    const { version, pinnedIds, orderedIds, hiddenIds } = decoded as Record<string, unknown>;
    const allowed =
      version === 2
        ? ['version', 'pinnedIds', 'orderedIds']
        : version === 3
          ? ['version', 'pinnedIds', 'orderedIds', 'hiddenIds']
          : [];
    if (
      fields.length !== allowed.length ||
      !fields.every((field) => allowed.includes(field)) ||
      !validIds(pinnedIds) ||
      !validIds(orderedIds) ||
      (version === 3 && !validIds(hiddenIds))
    )
      return emptyPreference();
    const hidden = version === 3 ? [...new Set(hiddenIds as string[])] : [];
    return {
      pinnedIds: [...new Set(pinnedIds)].filter((id) => !hidden.includes(id)),
      orderedIds: [...new Set(orderedIds)],
      hiddenIds: hidden,
    };
  } catch {
    return emptyPreference();
  }
}
export function parseHomePins(value: string | null): readonly string[] {
  return parseHomePreference(value).pinnedIds;
}

/** Orders an already authorized projection; unseen saved IDs never supply App content. */
export function orderHomeItems(
  items: readonly ItemDetail[],
  orderedIds: readonly string[],
): ItemDetail[] {
  const remaining = new Map(items.map((item) => [item.id, item]));
  const result: ItemDetail[] = [];
  for (const id of orderedIds) {
    const item = remaining.get(id);
    if (item) {
      result.push(item);
      remaining.delete(id);
    }
  }
  return [...result, ...remaining.values()];
}
/** Local shortcuts only: hiding never removes the canonical App or conversation content. */
export function projectHomeItems(items: readonly ItemDetail[], preference: HomePreference) {
  const allItems = orderHomeItems(items, preference.orderedIds);
  const hidden = new Set(preference.hiddenIds);
  return {
    allItems,
    items: allItems.filter((item) => !hidden.has(item.id)),
    hiddenItems: allItems.filter((item) => hidden.has(item.id)),
  };
}
export const sameOrder = (first: readonly string[], second: readonly string[]) =>
  first.length === second.length && first.every((id, index) => id === second[index]);
