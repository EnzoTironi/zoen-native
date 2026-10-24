import { reconcilePhotoSelection } from './photo-permission-reconciliation.ts';
import type { PhotoPickerPort, PhotoPickerState, Request } from './attachment-photo-types.ts';
type PhotoReloadOwner = {
  request: Request | null;
  state(): PhotoPickerState;
  current(request: Request): boolean;
  publish(patch: Partial<PhotoPickerState>): void;
  reload(requestPermission?: boolean): Promise<void>;
  port: PhotoPickerPort;
};
export async function reloadPhotos(owner: PhotoReloadOwner, requestPermission = false) {
  const request = owner.request;
  if (!request || !owner.current(request)) return;
  const epoch = ++request.epoch;
  owner.publish({ loading: true, error: null });
  try {
    let permission = await owner.port.permission();
    if (!owner.current(request) || epoch !== request.epoch) return;
    if (requestPermission && permission.status === 'undetermined')
      permission = await owner.port.requestPermission();
    if (!owner.current(request) || epoch !== request.epoch) return;
    const previousPermission = owner.state().permission;
    if (
      previousPermission &&
      (previousPermission.status !== permission.status ||
        previousPermission.accessPrivileges !== permission.accessPrivileges)
    )
      request.needsReconciliation = true;
    owner.publish({ permission });
    const retainedCopies = () =>
      (request.journal?.retained() ?? []).filter((item) =>
        owner.state().selected.some((selected) => selected.id === item.id),
      );
    if (!permission.granted) {
      request.needsReconciliation = false;
      owner.publish({
        items: [],
        selected: retainedCopies(),
        albums: [],
        hasMore: false,
        screen: 'grid',
        previewId: null,
      });
      return;
    }
    const reconciling = request.needsReconciliation;
    const retained = new Map(owner.state().selected.map((item) => [item.id, item]));
    const preview = owner.state().items.find((item) => item.id === owner.state().previewId);
    if (preview) retained.set(preview.id, preview);
    const [items, albums, refreshed] = await Promise.all([
      owner.port.load(owner.state().albumId, 0),
      owner.port.albums(),
      reconciling ? owner.port.refresh([...retained.values()]) : Promise.resolve([]),
    ]);
    if (!owner.current(request) || epoch !== request.epoch) return;
    if (reconciling) {
      // Permission can change during a metadata read. Never publish a result
      // under the privileges from a different authorization snapshot.
      const checked = await owner.port.permission();
      if (!owner.current(request) || epoch !== request.epoch) return;
      if (!checked.granted) {
        request.needsReconciliation = false;
        owner.publish({
          permission: checked,
          items: [],
          selected: retainedCopies(),
          albums: [],
          hasMore: false,
          screen: 'grid',
          previewId: null,
        });
        return;
      }
      if (
        checked.status !== permission.status ||
        checked.accessPrivileges !== permission.accessPrivileges
      ) {
        void owner.reload();
        return;
      }
      const durable = retainedCopies();
      const immutable = new Map(durable.map((item) => [item.id, item]));
      const verified = [
        ...refreshed.filter((item) => !immutable.has(item.id)),
        ...durable.filter((item) => retained.has(item.id)),
      ];
      const projection = reconcilePhotoSelection(owner.state(), retained, verified, items);
      if (!projection) {
        void owner.reload();
        return;
      }
      request.needsReconciliation = false;
      request.pageOffset = items.length;
      owner.publish({
        items: projection.items,
        albums,
        selected: projection.selected,
        hasMore: projection.hasMore,
        ...projection.preview,
      });
    } else {
      request.pageOffset = items.length;
      owner.publish({ items, albums, hasMore: items.length === 60 });
    }
  } catch (error) {
    if (owner.current(request) && epoch === request.epoch) owner.publish({ error });
  } finally {
    if (owner.current(request) && epoch === request.epoch) owner.publish({ loading: false });
  }
}
