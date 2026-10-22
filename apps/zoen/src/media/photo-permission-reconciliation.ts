import type { PhotoLibraryItem } from './attachment-photo-library';
import type { PhotoPickerState } from './attachment-photo-types';

/** Synchronous projection after the session has rechecked current permission and epoch. */
export function reconcilePhotoSelection(
  state: PhotoPickerState,
  retained: ReadonlyMap<string, PhotoLibraryItem>,
  refreshed: readonly PhotoLibraryItem[],
  items: readonly PhotoLibraryItem[],
): {
  items: readonly PhotoLibraryItem[];
  selected: readonly PhotoLibraryItem[];
  hasMore: boolean;
  preview: Partial<Pick<PhotoPickerState, 'screen' | 'previewId'>>;
} | null {
  const verified = new Map(refreshed.map((item) => [item.id, item]));
  if (verified.size !== refreshed.length || refreshed.some((item) => !retained.has(item.id)))
    throw new Error('Invalid Photos reconciliation');
  // A user may deselect/reorder while the read is in flight. Reconcile
  // CURRENT identities, never restore the selection captured before await.
  if (
    state.selected.some((item) => !retained.has(item.id)) ||
    (state.previewId !== null && !retained.has(state.previewId))
  ) {
    return null;
  }
  const selected = state.selected.flatMap((item) => {
    const current = verified.get(item.id);
    return current ? [current] : [];
  });
  const currentPreview = state.previewId === null ? null : verified.get(state.previewId);
  // The visible item may be beyond the refreshed first page. Retain only
  // its freshly authorized metadata, while the next offset stays based on
  // actual queried rows rather than this additional display identity.
  const reconciled = items.flatMap((item) => {
    if (retained.has(item.id) && !verified.has(item.id)) return [];
    return [verified.get(item.id) ?? item];
  });
  const displayed =
    currentPreview && !reconciled.some((item) => item.id === currentPreview.id)
      ? [...reconciled, currentPreview]
      : reconciled;
  const screen = state.screen;
  const lostPreview = state.previewId !== null && !currentPreview;

  return {
    items: displayed,
    selected,
    hasMore: items.length === 60,
    preview: lostPreview
      ? screen === 'selected' && selected.length
        ? { previewId: selected[0]!.id }
        : { screen: 'grid', previewId: null }
      : {},
  };
}
