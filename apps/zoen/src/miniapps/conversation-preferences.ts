import type { ItemDetail } from '../core/generated/ItemDetail';
import type { HomeOwner, HomePreferencesPort } from '../home/preferences';

export type ConversationCardPreference = Readonly<{
  hiddenIds: readonly string[];
  orderedIds: readonly string[];
}>;
export type ConversationPreferences = Readonly<Record<string, ConversationCardPreference>>;
export type ConversationCardsSource = {
  owner(): HomeOwner | null;
  spaces(): ReadonlySet<string>;
  items(): readonly ItemDetail[];
};
export type ConversationCards = Readonly<{
  all: readonly ItemDetail[];
  visible: readonly ItemDetail[];
  hiddenIds: readonly string[];
}>;
export class ConversationCardsFailure extends Error {
  readonly kind: 'load' | 'save' | 'accountChanged';
  constructor(kind: ConversationCardsFailure['kind'], cause?: unknown) {
    super(kind, { cause });
    this.kind = kind;
  }
}
export type ConversationCardsState = Readonly<{
  owner: HomeOwner | null;
  preferences: ConversationPreferences;
  loading: boolean;
  error: ConversationCardsFailure | null;
}>;

const MAX_SPACES = 100;
const MAX_IDS = 100;
const MAX_LENGTH = 1_048_576;
const validId = (value: unknown): value is string =>
  typeof value === 'string' &&
  /^[A-Za-z0-9_-]{1,128}$/.test(value) &&
  !['__proto__', 'prototype', 'constructor'].includes(value);
const emptyPreferences = (): ConversationPreferences => Object.create(null);
const emptyCards = (): ConversationCards => ({ all: [], visible: [], hiddenIds: [] });
const emptyState = (): ConversationCardsState => ({
  owner: null,
  preferences: emptyPreferences(),
  loading: false,
  error: null,
});
const validIds = (value: unknown): value is string[] =>
  Array.isArray(value) && value.length <= MAX_IDS && value.every(validId);

export function conversationOwnerKey(owner: HomeOwner | null): string | null {
  return owner
    ? `chat-cards:1:${JSON.stringify([owner.mode, owner.identityId, owner.deviceId])}`
    : null;
}

/** Preferences are untrusted local presentation hints, never cached miniapp content. */
export function parseConversationPreferences(value: string | null): ConversationPreferences {
  if (!value || value.length > MAX_LENGTH) return emptyPreferences();
  try {
    const decoded: unknown = JSON.parse(value);
    if (!decoded || typeof decoded !== 'object' || Array.isArray(decoded))
      return emptyPreferences();
    const entries = Object.entries(decoded);
    if (entries.length > MAX_SPACES) return emptyPreferences();
    const result: Record<string, ConversationCardPreference> = Object.create(null);
    for (const [spaceId, candidate] of entries) {
      if (
        !validId(spaceId) ||
        !candidate ||
        typeof candidate !== 'object' ||
        Array.isArray(candidate)
      )
        return emptyPreferences();
      const fields = Object.keys(candidate);
      if (fields.length !== 2 || !fields.includes('hiddenIds') || !fields.includes('orderedIds'))
        return emptyPreferences();
      const { hiddenIds, orderedIds } = candidate as Record<string, unknown>;
      if (!validIds(hiddenIds) || !validIds(orderedIds)) return emptyPreferences();
      result[spaceId] = {
        hiddenIds: [...new Set(hiddenIds)],
        orderedIds: [...new Set(orderedIds)],
      };
    }
    return result;
  } catch {
    return emptyPreferences();
  }
}

/** The canonical projection is supplied by Home; this controller never fetches or copies it. */
export function projectConversationCards(
  items: readonly ItemDetail[],
  spaces: ReadonlySet<string>,
  spaceId: string,
  preference?: ConversationCardPreference,
): ConversationCards {
  if (!validId(spaceId) || !spaces.has(spaceId)) return emptyCards();
  const canonical = new Map<string, ItemDetail>();
  for (const item of items) {
    if (
      item.app !== null &&
      item.space_id === spaceId &&
      validId(item.id) &&
      !canonical.has(item.id)
    )
      canonical.set(item.id, item);
  }
  const all: ItemDetail[] = [];
  for (const id of preference?.orderedIds ?? []) {
    const item = canonical.get(id);
    if (item) {
      all.push(item);
      canonical.delete(id);
    }
  }
  all.push(...canonical.values());
  const hidden = new Set(preference?.hiddenIds);
  return {
    all,
    visible: all.filter((item) => !hidden.has(item.id)),
    hiddenIds: all.filter((item) => hidden.has(item.id)).map((item) => item.id),
  };
}

/** Device-local rail choices are independent of Home favorites and canonical Items. */
export class ConversationCardsController {
  private state = emptyState();
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private disposed = false;
  private erasing = false;
  private cleanupOwner: HomeOwner | null = null;
  private restored = false;
  private restoreTask: Promise<void> | null = null;
  private projectionItems: readonly ItemDetail[] | null = null;
  private projectionSpaces: ReadonlySet<string> | null = null;
  private writes: Promise<void> = Promise.resolve();
  private readonly ownerWrites = new Map<string, Promise<void>>();
  private readonly source: ConversationCardsSource;
  private readonly port: HomePreferencesPort;

  constructor(source: ConversationCardsSource, port: HomePreferencesPort) {
    this.source = source;
    this.port = port;
  }
  getSnapshot = (): ConversationCardsState => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<ConversationCardsState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  private current(owner: HomeOwner, generation: number) {
    return (
      !this.disposed &&
      !this.erasing &&
      generation === this.generation &&
      conversationOwnerKey(this.source.owner()) === conversationOwnerKey(owner)
    );
  }
  private synchronize(): HomeOwner | null {
    const owner = this.source.owner();
    if (conversationOwnerKey(owner) !== conversationOwnerKey(this.state.owner)) {
      ++this.generation;
      this.restored = false;
      this.restoreTask = null;
      this.projectionItems = null;
      this.projectionSpaces = null;
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
    const items = this.source.items();
    const spaces = this.source.spaces();
    const projectionChanged =
      items !== this.projectionItems ||
      !this.projectionSpaces ||
      spaces.size !== this.projectionSpaces.size ||
      [...spaces].some((id) => !this.projectionSpaces!.has(id));
    this.projectionItems = items;
    if (projectionChanged) this.projectionSpaces = new Set(spaces);
    if (this.restored || this.restoreTask) {
      // Canonical items/membership may have changed without a preference write.
      if (projectionChanged && this.restored) this.publish({});
      return this.restoreTask ?? Promise.resolve();
    }
    const generation = this.generation;
    const priorWrites = this.ownerWrites.get(conversationOwnerKey(owner)!);
    this.publish({ loading: true, error: null });
    const restore = (async () => {
      try {
        // A previous session's accepted write may still finish for this device.
        // Read after it settles so returning to that owner cannot restore stale IDs.
        if (priorWrites) await priorWrites;
        if (!this.current(owner, generation)) return;
        const value =
          owner.mode === 'account' ? await this.port.get(conversationOwnerKey(owner)!) : null;
        if (!this.current(owner, generation)) return;
        this.restored = true;
        this.publish({
          preferences: parseConversationPreferences(value),
          loading: false,
          error: null,
        });
      } catch (cause) {
        if (this.current(owner, generation))
          this.publish({ loading: false, error: new ConversationCardsFailure('load', cause) });
      }
    })();
    this.restoreTask = restore;
    void restore.finally(() => {
      if (this.restoreTask === restore) this.restoreTask = null;
    });
    return restore;
  };
  cards = (spaceId: string): ConversationCards => {
    if (
      this.disposed ||
      this.erasing ||
      !this.restored ||
      !this.state.owner ||
      conversationOwnerKey(this.source.owner()) !== conversationOwnerKey(this.state.owner)
    )
      return emptyCards();
    return projectConversationCards(
      this.source.items(),
      this.source.spaces(),
      spaceId,
      this.state.preferences[spaceId],
    );
  };
  private write(
    spaceId: string,
    update: (
      cards: ConversationCards,
      preference: ConversationCardPreference,
    ) => ConversationCardPreference | null,
    capturedOwner: HomeOwner | null,
  ): Promise<void> {
    const owner = capturedOwner && { ...capturedOwner };
    const generation = this.generation;
    const work = this.writes.then(async () => {
      if (!owner || !this.current(owner, generation))
        throw new ConversationCardsFailure('accountChanged');
      let stage = 'readiness';
      try {
        if (!this.restored || !validId(spaceId) || !this.source.spaces().has(spaceId))
          throw new ConversationCardsFailure('save');
        const preference = this.state.preferences[spaceId] ?? { hiddenIds: [], orderedIds: [] };
        stage = 'projection';
        const cards = this.cards(spaceId);
        stage = 'update';
        const next = update(cards, preference);
        if (!next) return;
        stage = 'validation';
        if (!validIds(next.hiddenIds) || !validIds(next.orderedIds))
          throw new ConversationCardsFailure('save');
        const preferences: Record<string, ConversationCardPreference> = Object.assign(
          Object.create(null),
          this.state.preferences,
        );
        if (next.hiddenIds.length || next.orderedIds.length) preferences[spaceId] = next;
        else delete preferences[spaceId];
        stage = 'serialize';
        const serialized = JSON.stringify(preferences);
        if (Object.keys(preferences).length > MAX_SPACES || serialized.length > MAX_LENGTH)
          throw new ConversationCardsFailure('save');
        stage = 'storage';
        if (owner.mode === 'account') await this.port.set(conversationOwnerKey(owner)!, serialized);
        stage = 'commit';
        if (!this.current(owner, generation)) throw new ConversationCardsFailure('accountChanged');
        this.publish({ preferences, error: null });
      } catch (cause) {
        if (!this.current(owner, generation)) throw new ConversationCardsFailure('accountChanged');
        console.warn(
          '[ConversationCards] Save failed',
          stage,
          cause instanceof Error ? `${cause.name}: ${cause.message}` : typeof cause,
        );
        const error =
          cause instanceof ConversationCardsFailure
            ? cause
            : new ConversationCardsFailure('save', cause);
        this.publish({ error });
        throw error;
      }
    });
    this.writes = work.catch(() => {});
    if (owner) {
      const key = conversationOwnerKey(owner)!;
      const settled = this.writes;
      this.ownerWrites.set(key, settled);
      void settled.then(() => {
        if (this.ownerWrites.get(key) === settled) this.ownerWrites.delete(key);
      });
    }
    return work;
  }
  setPinned = (
    spaceId: string,
    itemId: string,
    pinned: boolean,
    owner: HomeOwner | null = this.state.owner,
  ): Promise<void> =>
    this.write(
      spaceId,
      (cards, preference) => {
        if (!validId(itemId) || !cards.all.some((item) => item.id === itemId))
          throw new ConversationCardsFailure('save');
        const hidden = preference.hiddenIds.includes(itemId);
        if (hidden === !pinned) return null;
        return {
          orderedIds: preference.orderedIds,
          hiddenIds: pinned
            ? preference.hiddenIds.filter((id) => id !== itemId)
            : [...preference.hiddenIds, itemId],
        };
      },
      owner,
    );
  reorder = (
    spaceId: string,
    ids: readonly string[],
    owner: HomeOwner | null = this.state.owner,
  ): Promise<void> => {
    const requested = [...ids];
    return this.write(
      spaceId,
      (cards, preference) => {
        if (!validIds(requested) || new Set(requested).size !== requested.length)
          throw new ConversationCardsFailure('save');
        const visible = cards.visible.map((item) => item.id);
        const visibleIds = new Set(visible);
        if (requested.length !== visible.length || requested.some((id) => !visibleIds.has(id)))
          throw new ConversationCardsFailure('save');
        if (requested.every((id, index) => id === visible[index])) return null;
        const ordered = [...preference.orderedIds];
        const saved = new Set(ordered);
        for (const item of cards.all) if (!saved.has(item.id)) ordered.push(item.id);
        let index = 0;
        return {
          hiddenIds: preference.hiddenIds,
          orderedIds: ordered.map((id) => (visibleIds.has(id) ? requested[index++]! : id)),
        };
      },
      owner,
    );
  };
  activate() {
    this.disposed = false;
    return this.refresh();
  }
  dispose() {
    this.disposed = true;
    ++this.generation;
    this.restored = false;
    this.restoreTask = null;
    this.projectionItems = null;
    this.projectionSpaces = null;
    this.state = emptyState();
  }
  async eraseDevice(): Promise<void> {
    this.erasing = true;
    ++this.generation;
    this.projectionItems = null;
    this.projectionSpaces = null;
    this.state = emptyState();
    this.listeners.forEach((listener) => listener());
    await this.writes;
    const owner = this.cleanupOwner;
    if (owner?.mode === 'account') await this.port.set(conversationOwnerKey(owner)!, '');
    this.cleanupOwner = null;
    this.erasing = false;
    this.restored = false;
    this.restoreTask = null;
  }
}
