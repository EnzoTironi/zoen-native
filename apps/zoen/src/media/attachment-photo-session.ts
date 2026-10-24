import type { AttachmentAsset } from './attachment-draft';
import type { PhotoLibraryItem } from './attachment-photo-library';
import {
  empty,
  type PhotoPickerPort,
  type PhotoPickerState,
  type Request,
} from './attachment-photo-types.ts';
import { reloadPhotos } from './attachment-photo-reload.ts';
import {
  canApprovePhotoSelection,
  reorderPhotoSelection,
  togglePhotoSelection,
} from './photo-selection.ts';
export type {
  PhotoPickerAdmission,
  PhotoPickerPort,
  PhotoPickerState,
  PhotoPickerSubmission,
} from './attachment-photo-types.ts';

/** Live Photos projection; the existing journal owns accepted private selection and captions. */
export class AttachmentPhotoSession {
  private state = empty;
  private request: Request | null = null;
  private sequence = 0;
  private previewOrigin: 'grid' | 'selected' = 'grid';
  private listeners = new Set<() => void>();
  private readonly port: PhotoPickerPort;
  constructor(port: PhotoPickerPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(patch: Partial<PhotoPickerState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  private current(request: Request) {
    return this.request === request && !request.retired && request.current();
  }
  isCurrent = (token: number) =>
    Boolean(token === this.state.token && this.request && this.current(this.request));
  isPresented = (token: number) => this.isCurrent(token) && this.state.visible;
  isInteractive = (token: number) => this.isPresented(token) && !this.state.busy;
  remaining = () => (this.request && this.current(this.request) ? this.request.remaining : 0);
  open(remaining: number, current: () => boolean, journal?: Request['journal']) {
    if (this.request || !current() || remaining < 1) return Promise.resolve([]);
    return new Promise<AttachmentAsset[]>((resolve, reject) => {
      const request: Request = {
        current,
        journal,
        remaining,
        resolve,
        reject,
        retired: false,
        working: false,
        epoch: 0,
        needsReconciliation: false,
        pageOffset: 0,
        action: null,
        result: null,
        albumDetails: new Map(),
      };
      this.request = request;
      this.state = empty;
      this.previewOrigin = 'grid';
      const restored = journal?.restore();
      this.publish({
        token: ++this.sequence,
        visible: true,
        ...(restored ? { selected: restored.selected, captions: restored.captions } : {}),
      });
      if (restored?.selected.length) request.needsReconciliation = true;
      void this.reload(true);
    });
  }
  private finish(request: Request, files: AttachmentAsset[]) {
    if (this.request !== request) {
      files.forEach((file) => this.port.discard(file.uri));
      return;
    }
    this.request = null;
    this.publish({ ...empty });
    request.resolve(files);
  }
  /** Lifecycle retirement preserves accepted private selections; explicit Cancel discards only this gallery's additions. */
  retire = () => this.close(false);
  cancel = () => this.close(true);
  private close(discard: boolean) {
    const request = this.request;
    if (!request || request.retired) return;
    request.retired = true;
    request.epoch++;
    this.publish({ visible: false });
    if (discard && request.journal) {
      request.discarding = true;
      const cleanup = request.journal.cancel();
      request.working = true;
      void cleanup.then(
        () => {
          request.discarding = false;
          this.finish(request, []);
        },
        (error) => {
          request.discarding = false;
          if (this.request === request) {
            this.request = null;
            this.publish({ ...empty });
            request.reject(error);
          }
        },
      );
    } else if (!request.working) {
      request.result?.forEach((file) => this.port.discard(file.uri));
      this.finish(request, []);
    }
  }
  private journalSelection() {
    const request = this.request;
    if (!request?.journal || !this.current(request)) return;
    void request.journal
      .update(this.state.selected, this.state.captions, this.port)
      .catch((error) => {
        if (this.current(request)) this.publish({ error });
      });
  }
  setCaptions(captions: PhotoPickerState['captions']) {
    if (!this.isInteractive(this.state.token)) return;
    this.publish({ captions });
    this.journalSelection();
  }

  reload(requestPermission = false): Promise<void> {
    return reloadPhotos(
      {
        request: this.request,
        state: this.getSnapshot,
        current: (request) => this.current(request),
        publish: (patch) => this.publish(patch),
        reload: (permission) => this.reload(permission),
        port: this.port,
      },
      requestPermission,
    );
  }

  async retry() {
    const request = this.request;
    if (!request || !this.current(request)) return;
    try {
      await request.journal?.retry();
      if (this.current(request)) await this.reload();
    } catch (error) {
      if (this.current(request)) this.publish({ error });
    }
  }
  invalidate = () => {
    if (this.request && this.current(this.request) && this.state.visible && !this.state.busy) {
      this.request.needsReconciliation = true;
      this.request.albumDetails.clear();
      void this.reload();
    }
  };
  async more() {
    const request = this.request;
    if (
      !request ||
      !this.current(request) ||
      !this.state.visible ||
      this.state.loading ||
      !this.state.hasMore
    )
      return;
    const epoch = request.epoch,
      offset = request.pageOffset;
    this.publish({ loading: true });
    try {
      const items = await this.port.load(this.state.albumId, offset);
      if (this.current(request) && epoch === request.epoch) {
        request.pageOffset += items.length;
        const ids = new Set(this.state.items.map((item) => item.id));
        this.publish({
          items: [...this.state.items, ...items.filter((item) => !ids.has(item.id))],
          hasMore: items.length === 60,
        });
      }
    } catch (error) {
      if (this.current(request) && epoch === request.epoch) this.publish({ error });
    } finally {
      if (this.current(request) && epoch === request.epoch) this.publish({ loading: false });
    }
  }
  album(id: string | null) {
    if (!this.request || !this.current(this.request) || !this.state.visible || this.state.busy)
      return;
    if (id !== null && !this.state.albums.some((album) => album.id === id)) return;
    this.publish({ albumId: id, items: [], screen: 'grid', previewId: null });
    void this.reload();
  }
  showGrid() {
    if (this.isInteractive(this.state.token)) this.publish({ screen: 'grid', previewId: null });
  }
  showCategory(category: PhotoPickerState['category']) {
    if (this.isInteractive(this.state.token) && !this.state.selected.length)
      this.publish({ category, screen: 'grid', previewId: null });
  }
  showAlbums() {
    if (this.isInteractive(this.state.token)) this.publish({ screen: 'albums' });
  }
  back() {
    if (!this.isInteractive(this.state.token)) return;
    if (
      this.state.screen === 'preview' &&
      this.previewOrigin === 'selected' &&
      this.state.selected.length
    )
      this.showSelected();
    else this.showGrid();
  }
  async albumDetails(id: string) {
    const request = this.request;
    const album = this.state.albums.find((item) => item.id === id);
    if (
      !request ||
      !this.isInteractive(this.state.token) ||
      this.state.screen !== 'albums' ||
      !album ||
      album.count !== undefined ||
      request.albumDetails.has(id) ||
      !this.port.albumDetails
    )
      return;
    const owner = {};
    const epoch = request.epoch;
    request.albumDetails.set(id, owner);
    try {
      const value = await this.port.albumDetails(id);
      if (
        !this.current(request) ||
        request.epoch !== epoch ||
        !Number.isSafeInteger(value.count) ||
        value.count < 0 ||
        (value.previewId !== null && !value.previewId.startsWith('ph://'))
      )
        return;
      this.publish({
        albums: this.state.albums.map((item) => (item.id === id ? { ...item, ...value } : item)),
      });
    } catch {
      // A single unavailable album thumbnail must not retire the Photos picker.
    } finally {
      if (request.albumDetails.get(id) === owner) request.albumDetails.delete(id);
    }
  }
  preview(id: string) {
    if (
      this.isInteractive(this.state.token) &&
      [...this.state.items, ...this.state.selected].some((item) => item.id === id)
    ) {
      if (this.state.screen !== 'preview')
        this.previewOrigin = this.state.screen === 'selected' ? 'selected' : 'grid';
      this.publish({ screen: 'preview', previewId: id });
    }
  }
  showSelected() {
    if (!this.isInteractive(this.state.token) || !this.state.selected.length) return;
    const previewId = this.state.selected.some((item) => item.id === this.state.previewId)
      ? this.state.previewId
      : this.state.selected[0]!.id;
    this.publish({ screen: 'selected', previewId });
  }
  reorder(ids: readonly string[]) {
    if (!this.isInteractive(this.state.token)) return;
    const selected = reorderPhotoSelection(this.state.selected, ids);
    if (selected) {
      this.publish({ selected });
      this.journalSelection();
    }
  }
  select(item: PhotoLibraryItem) {
    const request = this.request;
    const knownItem = [...this.state.items, ...this.state.selected].find(
      (value) => value.id === item.id,
    );
    if (!request || !this.current(request) || !this.state.visible || this.state.busy || !knownItem)
      return;
    const update = togglePhotoSelection(this.state, knownItem, request.remaining);
    if (update) {
      this.publish(update);
      this.journalSelection();
    }
  }
  async manage() {
    const request = this.request;
    if (!request || !this.current(request) || !this.state.visible || this.state.busy) return;
    try {
      await this.port.manage();
      if (this.current(request)) this.invalidate();
    } catch (error) {
      if (this.current(request)) this.publish({ error });
    }
  }
  canApprove(currentItemId?: string) {
    const request = this.request;
    if (!request || request.needsReconciliation || !this.isInteractive(this.state.token))
      return false;
    return canApprovePhotoSelection(this.state, request.remaining, currentItemId);
  }
  async approve(currentItemId?: string, prepared?: () => void) {
    const request = this.request;
    if (
      !request ||
      !this.current(request) ||
      !this.state.visible ||
      this.state.busy ||
      !this.canApprove(currentItemId)
    )
      return;
    // MediaPickerScreen.sendSelected selects the gallery's current item before
    // sending the ordered selection. Merely opening the gallery never selects it.
    if (currentItemId !== undefined) {
      const item = [...this.state.items, ...this.state.selected].find(
        (value) => value.id === currentItemId,
      );
      if (!item) return;
      if (!this.state.selected.some((value) => value.id === item.id)) {
        if (this.state.selected.length >= request.remaining) return;
        this.select(item);
      }
    }
    if (!this.state.selected.length) return;
    request.working = true;
    this.publish({ busy: true, error: null });
    let preparedFiles: AttachmentAsset[] | null = null;
    try {
      const result = await (request.journal
        ? request.journal.materialize(this.state.selected)
        : this.port.materialize(this.state.selected, () => this.current(request)));
      if (!this.current(request)) {
        result.forEach((file) => this.port.discard(file.uri));
        return;
      }
      preparedFiles = result;
      if (!result.length || result.length !== this.state.selected.length) {
        throw new Error('Attachment selection changed');
      }
      prepared?.();
      if (!this.current(request)) {
        result.forEach((file) => this.port.discard(file.uri));
        preparedFiles = null;
        return;
      }
      request.result = result;
      preparedFiles = null;
      this.publish({ visible: false });
    } catch (error) {
      preparedFiles?.forEach((file) => this.port.discard(file.uri));
      if (this.current(request)) this.publish({ error });
    } finally {
      request.working = false;
      if (request.retired && !request.discarding) this.finish(request, []);
      else this.publish({ busy: false });
    }
  }
  /** Another admitted child supplies owned selections, never source IDs/keys.
   * The same request still waits for actual native removal before resolving. */
  async prepareFiles(producer: (current: () => boolean) => Promise<AttachmentAsset[]>) {
    const request = this.request;
    if (
      !request ||
      !this.isInteractive(this.state.token) ||
      this.state.selected.length ||
      this.state.category === 'photos'
    )
      return;
    request.working = true;
    this.publish({ busy: true, error: null });
    try {
      const result = await producer(() => this.current(request));
      if (!this.current(request)) {
        result.forEach((file) => this.port.discard(file.uri));
        return;
      }
      if (!result.length || result.length > request.remaining) {
        result.forEach((file) => this.port.discard(file.uri));
        throw new Error('Attachment selection changed');
      }
      request.result = result;
      this.publish({ visible: false });
    } catch (error) {
      if (this.current(request)) this.publish({ error });
    } finally {
      request.working = false;
      if (request.retired && !request.discarding) this.finish(request, []);
      else this.publish({ busy: false });
    }
  }
  previewFile(producer: (current: () => boolean) => Promise<void>) {
    const request = this.request;
    if (!request || !this.isInteractive(this.state.token) || this.state.category === 'photos')
      return;
    request.action = 'recentPreview';
    request.preview = producer;
    this.publish({ visible: false });
  }
  camera() {
    const request = this.request;
    if (
      !request ||
      !this.current(request) ||
      !this.state.visible ||
      this.state.busy ||
      request.needsReconciliation ||
      this.state.selected.length >= request.remaining
    )
      return;
    request.action = 'camera';
    this.publish({ visible: false });
  }
  browse() {
    const request = this.request;
    if (
      !request ||
      !this.isInteractive(this.state.token) ||
      !this.port.browse ||
      (this.state.category !== 'document' && this.state.category !== 'audio') ||
      this.state.selected.length
    )
      return;
    request.action = 'document';
    this.publish({ visible: false });
  }
  /** Explicit empty-selection route handoff, only after the actual native owner closes. */
  openRoute(open: (current: () => boolean) => void) {
    const request = this.request;
    if (!request || !this.isInteractive(this.state.token) || this.state.selected.length) return;
    request.action = 'route';
    request.route = open;
    this.publish({ visible: false });
  }
  dismissing(token: number) {
    // User interactive dismissal revokes queued actions before the closing animation.
    // Our own approved/camera handoff has already hidden the sheet and must await onDismiss.
    if (token === this.state.token && this.state.visible) this.cancel();
  }
  async dismissed(token: number) {
    if (token !== this.state.token) return;
    if (!this.isCurrent(token)) {
      this.cancel();
      return;
    }
    const request = this.request;
    if (!request || request.retired || request.working) return;
    if (request.result) {
      const result = request.result;
      request.result = null;
      this.finish(request, result);
      return;
    }
    const action = request.action;
    if (action === 'route') {
      const open = request.route;
      request.route = undefined;
      request.action = null;
      try {
        if (this.current(request)) open?.(() => this.current(request));
      } catch (error) {
        if (this.current(request)) this.publish({ visible: true, error });
        return;
      }
      this.finish(request, []);
      return;
    }
    if (action === 'recentPreview') {
      const preview = request.preview;
      request.preview = undefined;
      request.action = null;
      request.working = true;
      this.publish({ busy: true });
      try {
        await preview?.(() => this.current(request));
      } catch (error) {
        if (this.current(request)) this.publish({ error });
      } finally {
        request.working = false;
        if (request.retired && !request.discarding) this.finish(request, []);
        else if (this.current(request)) this.publish({ busy: false, visible: true });
      }
      return;
    }
    if (action !== 'camera' && action !== 'document') {
      this.cancel();
      return;
    }
    request.action = null;
    request.working = true;
    this.publish({ busy: true });
    let captures: AttachmentAsset[] = [];
    try {
      captures =
        action === 'camera'
          ? await this.port.capture(request.remaining - this.state.selected.length, () =>
              this.current(request),
            )
          : await this.port.browse!(
              request.remaining,
              () => this.current(request),
              this.state.category === 'audio' ? 'audio' : 'document',
            );
      if (!this.current(request)) return;
      if (
        action === 'document' &&
        this.state.category === 'audio' &&
        captures.some((file) => !file.mime.startsWith('audio/'))
      )
        throw new Error('Audio file unavailable');
      if (!captures.length) {
        this.publish({ visible: true });
        return;
      }
      const selected = await (request.journal && this.state.selected.length
        ? request.journal.materialize(this.state.selected)
        : this.port.materialize(this.state.selected, () => this.current(request)));
      if (!this.current(request)) {
        selected.forEach((file) => this.port.discard(file.uri));
        return;
      }
      const all = [...selected, ...captures];
      captures = [];
      this.finish(request, all);
    } catch (error) {
      if (this.current(request)) this.publish({ visible: true, error });
    } finally {
      captures.forEach((file) => this.port.discard(file.uri));
      request.working = false;
      if (request.retired && !request.discarding) this.finish(request, []);
      else if (this.request === request) this.publish({ busy: false });
    }
  }
}
