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

const itemId = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const emptyState = (): HomeState => ({
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
const validIds = (value: unknown): value is string[] =>
  Array.isArray(value) && value.length <= 100 && value.every(itemId);
const maxPreferenceLength = 49_152;
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
const sameOrder = (first: readonly string[], second: readonly string[]) =>
  first.length === second.length && first.every((id, index) => id === second[index]);

/** Only local IDs persist. Every displayed miniapp is reread from the canonical core. */
export class HomeController {
  private state = emptyState();
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private layoutRevision = 0;
  private pendingWrites = 0;
  /** One bounded preview over the existing device preference; only Done writes its v3 key. */
  private draft: {
    owner: HomeOwner;
    generation: number;
    revision: string | number;
    access: string;
    baseline: HomePreference;
  } | null = null;
  private accessKey(items = this.state.items) {
    const spaces = this.source.spaces();
    return JSON.stringify([
      [...spaces].sort(),
      items
        .filter((item) => spaces.has(item.space_id))
        .map((item) => [item.id, item.space_id])
        .sort(),
    ]);
  }
  private draftCurrent(draft: NonNullable<HomeController['draft']>) {
    return (
      this.current(draft.owner, draft.generation) &&
      draft.revision === (this.source.revision?.() ?? 0) &&
      draft.access === this.accessKey()
    );
  }
  beginEdit = (owner: HomeOwner | null = this.state.owner): boolean => {
    if (this.draft) return !this.state.editSaving && this.draftCurrent(this.draft);
    if (
      !owner ||
      !this.current(owner, this.generation) ||
      !this.restored ||
      this.pendingWrites ||
      this.state.loading
    )
      return false;
    this.draft = {
      owner: { ...owner },
      generation: this.generation,
      revision: this.source.revision?.() ?? 0,
      access: this.accessKey(),
      baseline: {
        pinnedIds: this.state.pinnedIds,
        orderedIds: this.state.orderedIds,
        hiddenIds: this.state.hiddenIds,
      },
    };
    this.publish({ editing: true, error: null });
    return true;
  };
  discardEdit = (owner: HomeOwner | null = this.state.owner): void => {
    const draft = this.draft;
    if (!draft || this.state.editSaving || homeOwnerKey(owner) !== homeOwnerKey(draft.owner))
      return;
    this.draft = null;
    this.publish({ ...draft.baseline, editing: false, error: null });
  };
  commitEdit = (owner: HomeOwner | null = this.state.owner): Promise<void> => {
    const draft = this.draft;
    if (
      !draft ||
      this.state.editSaving ||
      this.pendingWrites ||
      !owner ||
      !this.draftCurrent(draft) ||
      homeOwnerKey(owner) !== homeOwnerKey(draft.owner) ||
      draft.access !== this.accessKey()
    )
      return Promise.reject(new HomeFailure('accountChanged'));
    const preference = {
      pinnedIds: this.state.pinnedIds,
      orderedIds: this.state.orderedIds,
      hiddenIds: this.state.hiddenIds,
    };
    if (
      sameOrder(preference.pinnedIds, draft.baseline.pinnedIds) &&
      sameOrder(preference.orderedIds, draft.baseline.orderedIds) &&
      sameOrder(preference.hiddenIds, draft.baseline.hiddenIds)
    ) {
      this.draft = null;
      this.publish({ editing: false, error: null });
      return Promise.resolve();
    }
    this.publish({ editSaving: true });
    return this.writePreference(
      () => {
        if (this.draft !== draft || !this.draftCurrent(draft)) throw new HomeFailure('save');
        return preference;
      },
      owner,
      true,
    )
      .then(() => {
        if (this.draft === draft) {
          this.draft = null;
          this.publish({ editing: false });
        }
      })
      .finally(() => {
        if (this.current(owner, draft.generation)) this.publish({ editSaving: false });
      });
  };
  requireEdit = (owner: HomeOwner | null = this.state.owner): void => {
    if (
      !this.draft ||
      !owner ||
      this.state.editSaving ||
      !this.draftCurrent(this.draft) ||
      homeOwnerKey(owner) !== homeOwnerKey(this.draft.owner) ||
      this.draft.access !== this.accessKey()
    )
      throw new HomeFailure('accountChanged');
  };
  private retireEdit() {
    if (this.draft && !this.state.editSaving) this.discardEdit(this.draft.owner);
  }
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
    if (
      (patch.orderedIds && !sameOrder(patch.orderedIds, this.state.orderedIds)) ||
      (patch.hiddenIds && !sameOrder(patch.hiddenIds, this.state.hiddenIds))
    )
      ++this.layoutRevision;
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
      this.draft = null;
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
    if (this.draft && !this.draftCurrent(this.draft)) this.retireEdit();
    const spaces = this.source.spaces();
    const accessible = this.state.items.filter((item) => spaces.has(item.space_id));
    if (accessible.length !== this.state.items.length) {
      this.retireEdit();
      this.publish({ items: accessible });
    }
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
          this.publish(parseHomePreference(value));
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
            const nextItems = items.filter((item) => {
              if (item.app === null || !spaces.has(item.space_id) || seen.has(item.id))
                return false;
              seen.add(item.id);
              return true;
            });
            const nextAccess = this.accessKey(nextItems);
            if (this.draft && this.draft.access !== nextAccess) this.retireEdit();
            this.publish({ items: nextItems });
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
  private writePreference(
    update: () => HomePreference | null,
    owner: HomeOwner | null,
    commit = false,
  ): Promise<void> {
    const generation = this.generation;
    const draftAtRequest = this.draft;
    ++this.pendingWrites;
    const work = this.writes.then(async () => {
      if (!owner || !this.current(owner, generation)) throw new HomeFailure('accountChanged');
      try {
        if (draftAtRequest && (this.draft !== draftAtRequest || !this.draftCurrent(draftAtRequest)))
          throw new HomeFailure('accountChanged');
        if (!this.restored) throw new HomeFailure('save');
        const preference = update();
        if (!preference) return;
        if (
          !validIds(preference.pinnedIds) ||
          !validIds(preference.orderedIds) ||
          !validIds(preference.hiddenIds)
        )
          throw new HomeFailure('save');
        const serialized = JSON.stringify({
          version: 3,
          pinnedIds: preference.pinnedIds,
          orderedIds: preference.orderedIds,
          hiddenIds: preference.hiddenIds,
        });
        if (serialized.length > maxPreferenceLength) throw new HomeFailure('save');
        if (this.draft && !commit) {
          if (this.state.editSaving || this.draft.access !== this.accessKey())
            throw new HomeFailure('save');
        } else if (owner.mode === 'account') await this.port.set(homeOwnerKey(owner)!, serialized);
        if (!this.current(owner, generation)) throw new HomeFailure('accountChanged');
        this.publish({ ...preference, error: null });
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
    return work.finally(() => {
      --this.pendingWrites;
    });
  }
  private writePins(update: () => readonly string[] | null, owner: HomeOwner | null) {
    return this.writePreference(() => {
      const pinnedIds = update();
      return pinnedIds
        ? { pinnedIds, orderedIds: this.state.orderedIds, hiddenIds: this.state.hiddenIds }
        : null;
    }, owner);
  }
  private requireAccessible(id: string) {
    if (
      !itemId(id) ||
      !this.state.items.some((item) => item.id === id && this.source.spaces().has(item.space_id))
    )
      throw new HomeFailure('save');
  }
  pin = (id: string, owner: HomeOwner | null = this.state.owner): Promise<void> =>
    this.writePreference(() => {
      this.requireAccessible(id);
      const pinned = this.state.pinnedIds.includes(id);
      const hidden = this.state.hiddenIds.includes(id);
      if (pinned && !hidden) return null;
      return {
        pinnedIds: pinned ? this.state.pinnedIds : [...this.state.pinnedIds, id],
        orderedIds: this.state.orderedIds,
        hiddenIds: this.state.hiddenIds.filter((value) => value !== id),
      };
    }, owner);
  unpin = (id: string, owner: HomeOwner | null = this.state.owner): Promise<void> =>
    this.writePins(() => {
      if (!itemId(id)) throw new HomeFailure('save');
      return this.state.pinnedIds.includes(id)
        ? this.state.pinnedIds.filter((value) => value !== id)
        : null;
    }, owner);
  hide = (id: string, owner: HomeOwner | null = this.state.owner): Promise<void> =>
    this.writePreference(() => {
      this.requireAccessible(id);
      if (this.state.hiddenIds.includes(id)) return null;
      return {
        pinnedIds: this.state.pinnedIds.filter((value) => value !== id),
        orderedIds: this.state.orderedIds,
        hiddenIds: [...this.state.hiddenIds, id],
      };
    }, owner);
  restore = (id: string, owner: HomeOwner | null = this.state.owner): Promise<void> =>
    this.writePreference(() => {
      this.requireAccessible(id);
      if (!this.state.hiddenIds.includes(id)) return null;
      return {
        pinnedIds: this.state.pinnedIds,
        orderedIds: this.state.orderedIds,
        hiddenIds: this.state.hiddenIds.filter((value) => value !== id),
      };
    }, owner);
  restoreAll = (owner: HomeOwner | null = this.state.owner): Promise<void> =>
    this.writePreference(() => {
      const spaces = this.source.spaces();
      const accessible = new Set(
        this.state.items.filter((item) => spaces.has(item.space_id)).map((item) => item.id),
      );
      const hiddenIds = this.state.hiddenIds.filter((id) => !accessible.has(id));
      if (hiddenIds.length === this.state.hiddenIds.length) return null;
      return { pinnedIds: this.state.pinnedIds, orderedIds: this.state.orderedIds, hiddenIds };
    }, owner);
  /** Reorder the complete visible selection; unavailable saved IDs keep their slots. */
  reorder = (
    ids: readonly string[],
    owner: HomeOwner | null = this.state.owner,
    expectedIds?: readonly string[],
  ): Promise<void> => {
    const requested = [...ids];
    const expected = expectedIds && [...expectedIds];
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
      if (expected && !sameOrder(expected, visible)) throw new HomeFailure('save');
      if (requested.length !== visible.length || requested.some((id) => !visibleIds.has(id)))
        throw new HomeFailure('save');
      if (requested.every((id, index) => id === visible[index])) return null;
      let index = 0;
      return this.state.pinnedIds.map((id) => (visibleIds.has(id) ? requested[index++]! : id));
    }, owner);
  };
  /** Reorder visible Home/inbox shortcuts; hidden and unavailable saved slots stay in place. */
  reorderItems = (
    ids: readonly string[],
    owner: HomeOwner | null = this.state.owner,
    expectedIds?: readonly string[],
  ): Promise<void> => {
    const requested = [...ids];
    const expected = expectedIds && [...expectedIds];
    const layoutRevision = this.layoutRevision;
    return this.writePreference(() => {
      if (layoutRevision !== this.layoutRevision) throw new HomeFailure('save');
      if (!validIds(requested) || new Set(requested).size !== requested.length)
        throw new HomeFailure('save');
      const spaces = this.source.spaces();
      const items = this.state.items.filter((item) => spaces.has(item.space_id));
      const visible = projectHomeItems(items, this.state).items.map((item) => item.id);
      const visibleIds = new Set(visible);
      if (expected && !sameOrder(expected, visible)) throw new HomeFailure('save');
      if (requested.length !== visible.length || requested.some((id) => !visibleIds.has(id)))
        throw new HomeFailure('save');
      if (sameOrder(requested, visible)) return null;
      const ordered = [...this.state.orderedIds];
      const saved = new Set(ordered);
      // Seed every authorized App before replacing visible positions, including hidden
      // Apps whose default positions have not previously been written to orderedIds.
      for (const item of orderHomeItems(items, this.state.orderedIds))
        if (!saved.has(item.id)) ordered.push(item.id);
      let index = 0;
      return {
        pinnedIds: this.state.pinnedIds,
        hiddenIds: this.state.hiddenIds,
        orderedIds: ordered.map((id) => (visibleIds.has(id) ? requested[index++]! : id)),
      };
    }, owner);
  };
  activate() {
    this.disposed = false;
    return this.refresh();
  }
  dispose() {
    this.retireEdit();
    this.disposed = true;
    this.draft = null;
    ++this.generation;
    this.restoreTask = null;
    this.itemsTask = null;
    this.itemsDirty = false;
    this.restored = false;
  }
  /** Retained cleanup ownership survives ClientStore clearing its account snapshot. */
  async eraseDevice(): Promise<void> {
    this.erasing = true;
    this.draft = null;
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
