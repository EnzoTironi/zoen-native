import type { AttachmentAsset } from './attachment-draft';
import type {
  PhotoLibraryAlbum,
  PhotoLibraryItem,
  PhotoLibraryPermission,
} from './attachment-photo-library';
export type PhotoPickerState = Readonly<{
  token: number;
  visible: boolean;
  busy: boolean;
  permission: PhotoLibraryPermission | null;
  items: readonly PhotoLibraryItem[];
  selected: readonly PhotoLibraryItem[];
  albums: readonly PhotoLibraryAlbum[];
  albumId: string | null;
  loading: boolean;
  hasMore: boolean;
  error: unknown;
}>;
export type PhotoPickerPort = {
  permission(): Promise<PhotoLibraryPermission>;
  requestPermission(): Promise<PhotoLibraryPermission>;
  load(albumId: string | null, offset: number): Promise<PhotoLibraryItem[]>;
  albums(): Promise<PhotoLibraryAlbum[]>;
  manage(): Promise<void>;
  materialize(
    items: readonly PhotoLibraryItem[],
    current: () => boolean,
  ): Promise<AttachmentAsset[]>;
  capture(remaining: number, current: () => boolean): Promise<AttachmentAsset[]>;
  discard(uri: string): void;
};
type Request = {
  current(): boolean;
  remaining: number;
  resolve(files: AttachmentAsset[]): void;
  retired: boolean;
  working: boolean;
  epoch: number;
  action: 'camera' | null;
  result: AttachmentAsset[] | null;
};
const empty: PhotoPickerState = {
  token: 0,
  visible: false,
  busy: false,
  permission: null,
  items: [],
  selected: [],
  albums: [],
  albumId: null,
  loading: false,
  hasMore: false,
  error: null,
};

/** Ephemeral library selection only. The existing channel remains the sole durable draft authority. */
export class AttachmentPhotoSession {
  private state = empty;
  private request: Request | null = null;
  private sequence = 0;
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
  open(remaining: number, current: () => boolean) {
    if (this.request || !current() || remaining < 1) return Promise.resolve([]);
    return new Promise<AttachmentAsset[]>((resolve) => {
      const request: Request = {
        current,
        remaining,
        resolve,
        retired: false,
        working: false,
        epoch: 0,
        action: null,
        result: null,
      };
      this.request = request;
      this.state = empty;
      this.publish({ token: ++this.sequence, visible: true });
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
  cancel = () => {
    const request = this.request;
    if (!request) return;
    request.retired = true;
    request.epoch++;
    this.publish({ visible: false });
    if (!request.working) {
      request.result?.forEach((file) => this.port.discard(file.uri));
      this.finish(request, []);
    }
  };
  async reload(requestPermission = false) {
    const request = this.request;
    if (!request || !this.current(request)) return;
    const epoch = ++request.epoch;
    this.publish({ loading: true, error: null });
    try {
      let permission = await this.port.permission();
      if (!this.current(request) || epoch !== request.epoch) return;
      if (requestPermission && permission.status === 'undetermined')
        permission = await this.port.requestPermission();
      if (!this.current(request) || epoch !== request.epoch) return;
      this.publish({ permission });
      if (!permission.granted) {
        this.publish({ items: [], selected: [], albums: [], hasMore: false });
        return;
      }
      const [items, albums] = await Promise.all([
        this.port.load(this.state.albumId, 0),
        this.port.albums(),
      ]);
      if (!this.current(request) || epoch !== request.epoch) return;
      this.publish({ items, albums, hasMore: items.length === 60 });
    } catch (error) {
      if (this.current(request) && epoch === request.epoch) this.publish({ error });
    } finally {
      if (this.current(request) && epoch === request.epoch) this.publish({ loading: false });
    }
  }
  invalidate = () => {
    if (this.request && this.state.visible && !this.state.busy) {
      this.publish({ selected: [] });
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
      offset = this.state.items.length;
    this.publish({ loading: true });
    try {
      const items = await this.port.load(this.state.albumId, offset);
      if (this.current(request) && epoch === request.epoch) {
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
    this.publish({ albumId: id, items: [] });
    void this.reload();
  }
  select(item: PhotoLibraryItem) {
    const request = this.request;
    if (
      !request ||
      !this.current(request) ||
      !this.state.visible ||
      this.state.busy ||
      !this.state.items.some((value) => value.id === item.id)
    )
      return;
    const existing = this.state.selected.some((value) => value.id === item.id);
    if (!existing && this.state.selected.length >= request.remaining) return;
    this.publish({
      selected: existing
        ? this.state.selected.filter((value) => value.id !== item.id)
        : [...this.state.selected, item],
    });
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
  async approve() {
    const request = this.request;
    if (
      !request ||
      !this.current(request) ||
      !this.state.visible ||
      this.state.busy ||
      !this.state.selected.length
    )
      return;
    request.working = true;
    this.publish({ busy: true, error: null });
    try {
      const result = await this.port.materialize(this.state.selected, () => this.current(request));
      if (!this.current(request)) {
        result.forEach((file) => this.port.discard(file.uri));
        return;
      }
      request.result = result;
      this.publish({ visible: false });
    } catch (error) {
      if (this.current(request)) this.publish({ error });
    } finally {
      request.working = false;
      if (request.retired) this.finish(request, []);
      else this.publish({ busy: false });
    }
  }
  camera() {
    const request = this.request;
    if (
      !request ||
      !this.current(request) ||
      !this.state.visible ||
      this.state.busy ||
      this.state.selected.length >= request.remaining
    )
      return;
    request.action = 'camera';
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
    if (request.action !== 'camera') {
      this.cancel();
      return;
    }
    request.action = null;
    request.working = true;
    this.publish({ busy: true });
    let captures: AttachmentAsset[] = [];
    try {
      captures = await this.port.capture(request.remaining - this.state.selected.length, () =>
        this.current(request),
      );
      if (!this.current(request)) return;
      if (!captures.length) {
        this.publish({ visible: true });
        return;
      }
      const selected = await this.port.materialize(this.state.selected, () =>
        this.current(request),
      );
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
      if (request.retired) this.finish(request, []);
      else if (this.request === request) this.publish({ busy: false });
    }
  }
}
