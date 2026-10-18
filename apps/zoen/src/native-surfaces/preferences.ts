import { sameSurfaceOwner, surfaceId, validSurfaceOwner } from './snapshot.ts';
import type { SurfaceOwner } from './snapshot';
import { MAX_WIDGET_SELECTIONS } from './widget-catalog.ts';

export type WidgetSelection = { itemId: string; showContent: boolean };
export type SurfacePreferences = {
  schemaVersion: 2;
  owner: SurfaceOwner;
  widgets: WidgetSelection[];
  shortcutItems: string[];
  activity: {
    id: string;
    linkId: string;
    itemId: string;
    endsAt: number;
    showContent: boolean;
  } | null;
};

export function emptyPreferences(owner: SurfaceOwner): SurfacePreferences {
  return { schemaVersion: 2, owner: { ...owner }, widgets: [], shortcutItems: [], activity: null };
}

const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

/** Device preferences only. No domain state, content, crypto material or provider credentials. */
export function parsePreferences(json: string | null, owner: SurfaceOwner): SurfacePreferences {
  const empty = emptyPreferences(owner);
  if (!owner || !validSurfaceOwner(owner) || !json || json.length > 8_192) return empty;
  try {
    const value: unknown = JSON.parse(json);
    if (
      !record(value) ||
      value.schemaVersion !== 2 ||
      !record(value.owner) ||
      typeof value.owner.identityId !== 'string' ||
      typeof value.owner.deviceId !== 'string' ||
      !sameSurfaceOwner(
        { identityId: value.owner.identityId, deviceId: value.owner.deviceId },
        owner,
      ) ||
      !Array.isArray(value.shortcutItems) ||
      value.shortcutItems.length > 20 ||
      value.shortcutItems.some((id) => !surfaceId(id))
    )
      return empty;
    const selections = value.widgets;
    if (
      !Array.isArray(selections) ||
      selections.length > MAX_WIDGET_SELECTIONS ||
      selections.some(
        (selection) =>
          !record(selection) ||
          !surfaceId(selection.itemId) ||
          typeof selection.showContent !== 'boolean',
      )
    )
      return empty;
    const widgets: WidgetSelection[] = [];
    const seen = new Set<string>();
    for (const selection of selections) {
      const { itemId, showContent } = selection as WidgetSelection;
      if (seen.has(itemId)) continue;
      seen.add(itemId);
      widgets.push({ itemId, showContent });
    }
    const activity = value.activity;
    if (
      activity !== null &&
      (!record(activity) ||
        !surfaceId(activity.id) ||
        !surfaceId(activity.linkId) ||
        !surfaceId(activity.itemId) ||
        typeof activity.endsAt !== 'number' ||
        !Number.isSafeInteger(activity.endsAt) ||
        activity.endsAt < 0 ||
        typeof activity.showContent !== 'boolean')
    )
      return empty;
    // Explicit reconstruction drops unexpected stored fields.
    return {
      ...empty,
      widgets,
      shortcutItems: [...new Set(value.shortcutItems as string[])],
      activity: record(activity)
        ? {
            id: activity.id as string,
            linkId: activity.linkId as string,
            itemId: activity.itemId as string,
            endsAt: activity.endsAt as number,
            showContent: activity.showContent as boolean,
          }
        : null,
    };
  } catch {
    return empty;
  }
}
