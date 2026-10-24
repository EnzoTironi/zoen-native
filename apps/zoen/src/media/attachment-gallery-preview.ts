import type { PhotoPickerPort, PhotoPickerState, Request } from './attachment-photo-types';
/** Display leases are replaceable projections of the SAME encrypted journal assets used by Send. */
export function prepareGalleryPreview(
  owner: {
    request: Request;
    state(): PhotoPickerState;
    current(): boolean;
    publish(patch: Partial<PhotoPickerState>): void;
    port: PhotoPickerPort;
  },
  save: boolean,
) {
  const { request } = owner;
  const journal = request.journal;
  if (!journal || !owner.current()) return;
  const selected = owner.state().selected;
  const captions = owner.state().captions;
  const pending = Object.fromEntries(
    selected.map((item) => [item.id, owner.state().previewUris[item.id] ?? null]),
  );
  if (JSON.stringify(pending) !== JSON.stringify(owner.state().previewUris))
    owner.publish({ previewUris: pending });
  const currentSelection = () =>
    owner.current() &&
    selected.length === owner.state().selected.length &&
    selected.every((item, i) => item.id === owner.state().selected[i]?.id);
  const task = save ? journal.update(selected, captions, owner.port) : Promise.resolve();
  void task
    .then(async () => {
      if (!currentSelection() || !selected.length) return;
      const previewUris = await journal.preview(selected);
      if (!currentSelection()) return;
      if (JSON.stringify(previewUris) !== JSON.stringify(owner.state().previewUris))
        owner.publish({ previewUris });
    })
    .catch((error) => {
      if (currentSelection()) owner.publish({ error });
    });
}
