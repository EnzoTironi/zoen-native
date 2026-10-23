import type { PhotoLibraryItem } from './attachment-photo-library';
import type { PhotoPickerState } from './attachment-photo-types';

/** Pure selection projection; the session checks current request/permission first. */
export function reorderPhotoSelection(
  selected: readonly PhotoLibraryItem[],
  ids: readonly string[],
): PhotoLibraryItem[] | null {
  if (ids.length !== selected.length || new Set(ids).size !== ids.length) return null;
  const items = new Map(selected.map((item) => [item.id, item]));
  if (ids.some((id) => !items.has(id))) return null;
  return ids.map((id) => items.get(id)!);
}

export function togglePhotoSelection(
  state: PhotoPickerState,
  item: PhotoLibraryItem,
  remaining: number,
): Partial<PhotoPickerState> | null {
  const existing = state.selected.some((value) => value.id === item.id);
  if (!existing && state.selected.length >= remaining) return null;
  const selected = existing
    ? state.selected.filter((value) => value.id !== item.id)
    : [...state.selected, item];
  return {
    selected,
    ...(state.screen === 'selected'
      ? {
          screen: selected.length ? 'selected' : 'grid',
          previewId: selected.some((value) => value.id === state.previewId)
            ? state.previewId
            : (selected[0]?.id ?? null),
        }
      : {}),
  };
}

export function canApprovePhotoSelection(
  state: PhotoPickerState,
  remaining: number,
  currentItemId?: string,
): boolean {
  if (currentItemId === undefined) return state.selected.length > 0;
  if (state.selected.some((item) => item.id === currentItemId)) return true;
  return state.selected.length < remaining && state.items.some((item) => item.id === currentItemId);
}
