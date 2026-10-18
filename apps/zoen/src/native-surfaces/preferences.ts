import { sameSurfaceOwner, surfaceId, validSurfaceOwner } from './snapshot.ts';
import type { SurfaceOwner } from './snapshot';

export type SurfacePreferences = {
  schemaVersion: 1;
  owner: SurfaceOwner;
  widget: { itemId: string; showContent: boolean } | null;
  shortcutItems: string[];
  activity: {
    id: string;
    itemId: string;
    endsAt: number;
    showContent: boolean;
  } | null;
};

export function emptyPreferences(owner: SurfaceOwner): SurfacePreferences {
  return { schemaVersion: 1, owner: { ...owner }, widget: null, shortcutItems: [], activity: null };
}

/** Device preferences only. No domain state, content, crypto material or provider credentials. */
export function parsePreferences(json: string | null, owner: SurfaceOwner): SurfacePreferences {
  const empty = emptyPreferences(owner);
  if (!validSurfaceOwner(owner) || !json || json.length > 8_192) return empty;
  try {
    const value = JSON.parse(json) as SurfacePreferences;
    if (
      value?.schemaVersion !== 1 ||
      !value.owner ||
      !sameSurfaceOwner(value.owner, owner) ||
      !Array.isArray(value.shortcutItems) ||
      value.shortcutItems.length > 20 ||
      value.shortcutItems.some((id) => !surfaceId(id))
    )
      return empty;
    const widget = value.widget;
    if (
      widget !== null &&
      (!widget || !surfaceId(widget.itemId) || typeof widget.showContent !== 'boolean')
    )
      return empty;
    const activity = value.activity;
    if (
      activity !== null &&
      (!activity ||
        !surfaceId(activity.id) ||
        !surfaceId(activity.itemId) ||
        !Number.isSafeInteger(activity.endsAt) ||
        activity.endsAt < 0 ||
        typeof activity.showContent !== 'boolean')
    )
      return empty;
    // Explicit reconstruction also drops any unexpected stored fields.
    return {
      ...empty,
      widget: widget ? { itemId: widget.itemId, showContent: widget.showContent } : null,
      shortcutItems: [...new Set(value.shortcutItems)],
      activity: activity
        ? {
            id: activity.id,
            itemId: activity.itemId,
            endsAt: activity.endsAt,
            showContent: activity.showContent,
          }
        : null,
    };
  } catch {
    return empty;
  }
}
