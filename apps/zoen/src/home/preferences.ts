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
export type HomeState = Readonly<{
  owner: HomeOwner | null;
  items: ItemDetail[];
  pinnedIds: readonly string[];
  loading: boolean;
  error: HomeFailure | null;
}>;

const itemId = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const emptyState = (): HomeState => ({
  owner: null,
  items: [],
  pinnedIds: [],
  loading: false,
  error: null,
});
export function homeOwnerKey(owner: HomeOwner | null): string | null {
  return owner
    ? `home-pins:1:${JSON.stringify([owner.mode, owner.identityId, owner.deviceId])}`
    : null;
}
export function parseHomePins(value: string | null): readonly string[] {
  if (!value || value.length > 16_384) return [];
  try {
    const ids: unknown = JSON.parse(value);
    if (!Array.isArray(ids) || ids.length > 100 || !ids.every(itemId)) return [];
    return [...new Set(ids)];
  } catch {
    return [];
  }
}

/** Only local IDs persist. Every displayed miniapp is reread from the canonical core. */
export class HomeController {
  private state = emptyState();
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private disposed = false;
  private erasing = false;
  private cleanupOwner: HomeOwner | null = null;
  private restored = false;
  private restoreTask: Promise<void> | null = null;
  private itemsDirty = false;
  private itemsTask: Promise<void> | null = null;
  private writes: Promise<void> = Promise.resolve();
  private readonly ownerWrites = new Map<string, Promise<void>>();
  private readonly source: HomeSource;
  private readonly port: HomePreferencesPort;

  constructor(source: HomeSource, port: HomePreferencesPort) {
    this.source = source;
    this.port = port;
  }
  getSnapshot = (): HomeState => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<HomeState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  private current(owner: HomeOwner, generation: number) {
    return (
      !this.disposed &&
      !this.erasing &&
      generation === this.generation &&
      homeOwnerKey(this.source.owner()) === homeOwnerKey(owner)
    );
  }
  private synchronize(): HomeOwner | null {
    const owner = this.source.owner();
    if (homeOwnerKey(owner) !== homeOwnerKey(this.state.owner)) {
      ++this.generation;
      this.restored = false;
      this.restoreTask = null;
      this.itemsTask = null;
      this.itemsDirty = false;
      if (owner) this.cleanupOwner = { ...owner };
      this.state = { ...emptyState(), owner: owner && { ...owner }, loading: !!owner };
      this.listeners.forEach((listener) => listener());
    }
    return owner;
  }
  refresh = (): Promise<void> => {
    if (this.disposed || this.erasing) return Promise.resolve();
    const owner = this.synchronize();
    if (!owner) return Promise.resolve();
    const spaces = this.source.spaces();
    const accessible = this.state.items.filter((item) => spaces.has(item.space_id));
    if (accessible.length !== this.state.items.length) this.publish({ items: accessible });
    const generation = this.generation;
    if (!this.restored && !this.restoreTask) {
      const priorWrites = this.ownerWrites.get(homeOwnerKey(owner)!);
      const restore = (async () => {
        try {
          // A previous session's accepted write may still finish for this device.
          // Read after it settles so returning to that owner cannot restore stale IDs.
          if (priorWrites) await priorWrites;
          if (!this.current(owner, generation)) return;
          // Demo selections are intentionally volatile, even on a signed-in device.
          const value = owner.mode === 'account' ? await this.port.get(homeOwnerKey(owner)!) : null;
          if (!this.current(owner, generation)) return;
          this.restored = true;
          this.publish({ pinnedIds: parseHomePins(value) });
        } catch (cause) {
          if (this.current(owner, generation))
            this.publish({ error: new HomeFailure('load', cause) });
        }
      })();
      this.restoreTask = restore;
      void restore.finally(() => {
        if (this.restoreTask === restore) {
          this.restoreTask = null;
          this.publish({ loading: !!this.itemsTask });
        }
      });
    }
    this.itemsDirty = true;
    if (!this.itemsTask) {
      this.publish({ loading: true, error: null });
      const read = (async () => {
        while (this.itemsDirty && this.current(owner, generation)) {
          this.itemsDirty = false;
          try {
            const items = await this.source.items();
            if (!this.current(owner, generation)) return;
            if (this.itemsDirty) continue;
            const spaces = this.source.spaces();
            const seen = new Set<string>();
            this.publish({
              items: items.filter((item) => {
                if (item.app === null || !spaces.has(item.space_id) || seen.has(item.id))
                  return false;
                seen.add(item.id);
                return true;
              }),
            });
          } catch (cause) {
            if (this.current(owner, generation))
              this.publish({ items: [], error: new HomeFailure('load', cause) });
          }
        }
      })();
      this.itemsTask = read;
      void read.finally(() => {
        if (this.itemsTask === read) {
          this.itemsTask = null;
          this.publish({ loading: !!this.restoreTask });
        }
      });
    }
    return Promise.all([this.restoreTask, this.itemsTask]).then(() => {});
  };
  private writePins(
    update: () => readonly string[] | null,
    owner: HomeOwner | null,
  ): Promise<void> {
    const generation = this.generation;
    const work = this.writes.then(async () => {
      if (!owner || !this.current(owner, generation)) throw new HomeFailure('accountChanged');
      try {
        if (!this.restored) throw new HomeFailure('save');
        const ids = update();
        if (!ids) return;
        if (ids.length > 100) throw new HomeFailure('save');
        if (owner.mode === 'account')
          await this.port.set(homeOwnerKey(owner)!, JSON.stringify(ids));
        if (!this.current(owner, generation)) throw new HomeFailure('accountChanged');
        this.publish({ pinnedIds: ids, error: null });
      } catch (cause) {
        if (!this.current(owner, generation)) throw new HomeFailure('accountChanged');
        const error = cause instanceof HomeFailure ? cause : new HomeFailure('save', cause);
        this.publish({ error });
        throw error;
      }
    });
    this.writes = work.catch(() => {});
    if (owner) {
      const key = homeOwnerKey(owner)!;
      const settled = this.writes;
      this.ownerWrites.set(key, settled);
      void settled.then(() => {
        if (this.ownerWrites.get(key) === settled) this.ownerWrites.delete(key);
      });
    }
    return work;
  }
  private change(id: string, pinned: boolean, owner: HomeOwner | null): Promise<void> {
    return this.writePins(() => {
      if (!itemId(id)) throw new HomeFailure('save');
      if (
        pinned &&
        !this.state.items.some((item) => item.id === id && this.source.spaces().has(item.space_id))
      )
        throw new HomeFailure('save');
      const ids = this.state.pinnedIds.filter((value) => value !== id);
      if (pinned && !this.state.pinnedIds.includes(id)) ids.push(id);
      else if (pinned) return null;
      return ids;
    }, owner);
  }
  pin = (id: string, owner: HomeOwner | null = this.state.owner) => this.change(id, true, owner);
  unpin = (id: string, owner: HomeOwner | null = this.state.owner) => this.change(id, false, owner);
  /** Reorder the complete visible selection; unavailable saved IDs keep their slots. */
  reorder = (ids: readonly string[], owner: HomeOwner | null = this.state.owner): Promise<void> => {
    const requested = [...ids];
    return this.writePins(() => {
      if (
        requested.length > 100 ||
        !requested.every(itemId) ||
        new Set(requested).size !== requested.length
      )
        throw new HomeFailure('save');
      const spaces = this.source.spaces();
      const accessible = new Set(
        this.state.items.filter((item) => spaces.has(item.space_id)).map((item) => item.id),
      );
      const visible = this.state.pinnedIds.filter((id) => accessible.has(id));
      const visibleIds = new Set(visible);
      if (requested.length !== visible.length || requested.some((id) => !visibleIds.has(id)))
        throw new HomeFailure('save');
      if (requested.every((id, index) => id === visible[index])) return null;
      let index = 0;
      return this.state.pinnedIds.map((id) => (visibleIds.has(id) ? requested[index++]! : id));
    }, owner);
  };
  activate() {
    this.disposed = false;
    return this.refresh();
  }
  dispose() {
    this.disposed = true;
    ++this.generation;
    this.restoreTask = null;
    this.itemsTask = null;
    this.itemsDirty = false;
    this.restored = false;
  }
  /** Retained cleanup ownership survives ClientStore clearing its account snapshot. */
  async eraseDevice(): Promise<void> {
    this.erasing = true;
    ++this.generation;
    this.state = emptyState();
    this.listeners.forEach((listener) => listener());
    await this.writes;
    const owner = this.cleanupOwner;
    if (owner?.mode === 'account') await this.port.set(homeOwnerKey(owner)!, '');
    this.cleanupOwner = null;
    this.erasing = false;
    this.restored = false;
    this.restoreTask = null;
    this.itemsTask = null;
  }
}
