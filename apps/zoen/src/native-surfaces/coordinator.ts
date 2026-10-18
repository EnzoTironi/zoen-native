import { t } from '../i18n/index.ts';
import type { AppIntentEntity } from 'expo-app-intents';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { activityContent } from './activity.ts';
import type { MiniappActivityContent } from './activity';
import { itemEntityCatalog } from './catalog.ts';
import { surfaceActivityURL, surfaceItemURL } from './links.ts';
import { parsePreferences } from './preferences.ts';
import type { SurfacePreferences } from './preferences';
import { sameSurfaceOwner, widgetProjection } from './snapshot.ts';
import type { NativeWidgetContent, SnapshotItem, SurfaceOwner } from './snapshot';

export interface SurfacePort {
  readPreferences(): Promise<string | null>;
  eraseMetadata(): Promise<void>;
  writePreferences(value: string | null): Promise<void>;
  widget(content: NativeWidgetContent | null): void;
  catalogs(items: AppIntentEntity[], chats: AppIntentEntity[]): Promise<void>;
  activities(): string[];
  startActivity(
    content: MiniappActivityContent,
    owner: SurfaceOwner,
    itemId: string,
  ): { id: string; linkId: string };
  updateActivity(id: string, content: MiniappActivityContent): Promise<void>;
  endActivity(id: string): Promise<void>;
}
export interface SurfaceSource {
  loading(): boolean;
  owner(): SurfaceOwner | null;
  spaces(): ReadonlyMap<string, string>;
  item(id: string): Promise<ItemDetail>;
}
export type SurfaceState = Readonly<{
  ready: boolean;
  busy: boolean;
  preferences: SurfacePreferences | null;
  error: string | null;
}>;

/** Serializes publications and opt-ins; all displayed values are reread from the one core. */
export class SurfaceCoordinator {
  private state: SurfaceState = { ready: false, busy: false, preferences: null, error: null };
  private readonly listeners = new Set<() => void>();
  private tail: Promise<void> = Promise.resolve();
  private dirty = false;
  private active = false;
  private generation = 0;
  private cleanupPending = false;
  private preferencesCleanupPending = false;
  private metadataCleanupPending = false;
  private readonly source: SurfaceSource;
  private readonly port: SurfacePort;
  constructor(source: SurfaceSource, port: SurfacePort) {
    this.source = source;
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<SurfaceState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  activate() {
    this.active = true;
    void this.refresh();
  }
  dispose() {
    this.active = false;
    ++this.generation;
  }
  private current(owner: SurfaceOwner, generation = this.generation) {
    return (
      this.active && generation === this.generation && sameSurfaceOwner(this.source.owner(), owner)
    );
  }
  /** Only the still-running selected session may follow its current canonical item. */
  canOpenActivity = (item: SnapshotItem, linkId: string): boolean => {
    const preferences = this.state.preferences;
    const activity = preferences?.activity;
    if (
      !preferences ||
      !activity ||
      !this.state.ready ||
      this.state.busy ||
      this.cleanupPending ||
      this.source.loading() ||
      !this.current(preferences.owner) ||
      activity.linkId !== linkId ||
      activity.itemId !== item.id ||
      !this.source.spaces().has(item.space_id)
    )
      return false;
    const content = activityContent(item, preferences.owner);
    return (
      !!content &&
      content.endsAt === activity.endsAt &&
      this.port.activities().includes(activity.id)
    );
  };
  private queue<T>(work: () => Promise<T>): Promise<T> {
    const task = this.tail.then(work);
    this.tail = task.then(
      () => {},
      () => {},
    );
    return task;
  }
  refresh = (): Promise<void> => {
    this.dirty = true;
    return this.queue(async () => {
      while (this.dirty && this.active) {
        this.dirty = false;
        try {
          await this.reconcile();
        } catch {
          // Do not retain an unverified private projection after access/read failure.
          await this.clear();
          this.publish({ error: t('nativeSurfaces.iOSSurfacesCouldNotBeUpdated') });
        }
      }
    });
  };
  private async save(preferences: SurfacePreferences) {
    if (!this.current(preferences.owner)) throw new Error(t('nativeSurfaces.theAccountHasChanged'));
    await this.port.writePreferences(JSON.stringify(preferences));
    if (!this.current(preferences.owner)) throw new Error(t('nativeSurfaces.theAccountHasChanged'));
    this.publish({ preferences });
  }
  /** Every target gets a cleanup attempt even when an unrelated native bridge fails. */
  private async clear(resetPreferences = false): Promise<boolean> {
    this.cleanupPending = true;
    this.preferencesCleanupPending ||= resetPreferences;
    this.metadataCleanupPending ||= resetPreferences;
    if (this.preferencesCleanupPending) this.publish({ ready: false, preferences: null });
    let failed = false;
    const attempt = async (work: () => void | Promise<void>) => {
      try {
        await work();
      } catch {
        failed = true;
      }
    };
    await attempt(() => this.port.widget(null));
    await attempt(() => this.port.catalogs([], []));
    let activities: string[] = [];
    await attempt(() => {
      activities = this.port.activities();
    });
    for (const id of activities) await attempt(() => this.port.endActivity(id));
    if (this.metadataCleanupPending) {
      await attempt(async () => {
        await this.port.eraseMetadata();
        this.metadataCleanupPending = false;
      });
    }
    if (this.preferencesCleanupPending) {
      await attempt(async () => {
        await this.port.writePreferences(null);
        this.preferencesCleanupPending = false;
      });
    }
    this.cleanupPending = failed;
    return !failed;
  }
  eraseDevice(): Promise<void> {
    ++this.generation;
    this.publish({ ready: false, preferences: null });
    return this.queue(async () => {
      if (!(await this.clear(true)))
        throw new Error(t('nativeSurfaces.iOSPreviewsCouldNotBeErasedTryAgain'));
    });
  }
  private async reconcile() {
    // Unknown session at cold startup is not an authoritative empty account/catalog.
    if (this.source.loading()) return;
    const owner = this.source.owner();
    if (!owner) {
      if (!(await this.clear(true))) throw new Error('Native surface cleanup failed');
      this.publish({ ready: false, preferences: null, error: null });
      return;
    }
    const generation = this.generation;
    if (this.cleanupPending && !(await this.clear()))
      throw new Error('Native surface cleanup failed');
    if (!this.current(owner, generation)) return;
    let preferences = this.state.preferences;
    if (!preferences || !sameSurfaceOwner(preferences.owner, owner)) {
      if (preferences && !(await this.clear(true)))
        throw new Error('Native surface cleanup failed');
      if (!this.current(owner, generation)) return;
      preferences = parsePreferences(await this.port.readPreferences(), owner);
      if (!this.current(owner, generation)) return;
      this.publish({ preferences, ready: true });
    }
    for (const id of this.port.activities()) {
      if (id !== preferences.activity?.id) await this.port.endActivity(id);
      if (!this.current(owner, generation)) return;
    }
    const spaces = this.source.spaces();
    const ids = [
      ...new Set([
        ...preferences.shortcutItems,
        ...(preferences.widget ? [preferences.widget.itemId] : []),
        ...(preferences.activity ? [preferences.activity.itemId] : []),
      ]),
    ];
    const items: ItemDetail[] = [];
    for (const id of ids) {
      let item: ItemDetail;
      try {
        item = await this.source.item(id);
      } catch {
        continue;
      }
      if (!this.current(owner, generation)) return;
      if (item.id === id && spaces.has(item.space_id)) items.push(item);
    }
    if (!this.current(owner, generation)) return;
    const byId = new Map(items.map((item) => [item.id, item]));
    const widgetItem = preferences.widget && byId.get(preferences.widget.itemId);
    const projection =
      widgetItem && widgetProjection(widgetItem, owner, preferences.widget!.showContent);
    this.port.widget(
      projection
        ? {
            ...projection.content,
            deepLink: surfaceItemURL(owner, projection.itemId, projection.version),
          }
        : null,
    );
    const selected = new Set(preferences.shortcutItems);
    const catalog = itemEntityCatalog(items, owner, selected, new Set(spaces.keys()));
    const chatIds = new Set(
      items.filter((item) => selected.has(item.id)).map((item) => item.space_id),
    );
    const chats: AppIntentEntity[] = [...chatIds]
      .map((id) => ({
        id,
        title: spaces.get(id)!.slice(0, 80),
        hideInSpotlight: true,
        metadata: { ...owner },
      }))
      .filter((entity) => entity.title.trim().length > 0);
    await this.port.catalogs(catalog, chats);
    if (!this.current(owner, generation)) return;
    const activity = preferences.activity;
    const running = this.port.activities();
    for (const id of running) {
      if (id !== activity?.id) await this.port.endActivity(id);
      if (!this.current(owner, generation)) return;
    }
    if (activity) {
      const item = byId.get(activity.itemId);
      const content = item && activityContent(item, owner, activity.showContent);
      // A changed canonical end is a new event and needs another explicit user start.
      if (!content || content.endsAt !== activity.endsAt || !running.includes(activity.id)) {
        if (running.includes(activity.id)) await this.port.endActivity(activity.id);
        if (this.current(owner, generation)) await this.save({ ...preferences, activity: null });
      } else
        await this.port.updateActivity(activity.id, {
          ...content,
          deepLink: surfaceActivityURL(owner, activity.itemId, activity.linkId),
        });
    }
    if (this.current(owner, generation)) this.publish({ ready: true, error: null });
  }
  private async change(work: (preferences: SurfacePreferences) => Promise<void>) {
    return this.queue(async () => {
      const owner = this.source.owner();
      if (!owner || !this.current(owner))
        throw new Error(t('nativeSurfaces.openYourAccountBeforeContinuing'));
      this.publish({ busy: true, error: null });
      try {
        const preferences = sameSurfaceOwner(this.state.preferences?.owner ?? null, owner)
          ? this.state.preferences!
          : parsePreferences(await this.port.readPreferences(), owner);
        if (!this.current(owner)) throw new Error(t('nativeSurfaces.theAccountHasChanged'));
        await work(preferences);
        await this.reconcile();
      } catch (error) {
        this.publish({
          error:
            error instanceof Error ? error.message : t('nativeSurfaces.thisCouldNotBeCompleted'),
        });
        throw error;
      } finally {
        this.publish({ busy: false });
      }
    });
  }
  private async checkedItem(id: string, version: number, owner: SurfaceOwner) {
    const generation = this.generation;
    const item = await this.source.item(id);
    if (
      !this.current(owner, generation) ||
      item.id !== id ||
      !this.source.spaces().has(item.space_id)
    )
      throw new Error(t('nativeSurfaces.thisItemIsNoLongerAvailableInThis'));
    if (item.version !== version)
      throw new Error(t('nativeSurfaces.theMiniappHasChangedRefreshBeforeContinuing'));
    if (!widgetProjection(item, owner))
      throw new Error(t('nativeSurfaces.thisMiniappHasNoCompatiblePreview'));
    return item;
  }
  selectWidget(id: string, version: number, showContent: boolean) {
    return this.change(async (preferences) => {
      await this.checkedItem(id, version, preferences.owner);
      await this.save({ ...preferences, widget: { itemId: id, showContent } });
    });
  }
  removeWidget() {
    return this.change(async (preferences) => this.save({ ...preferences, widget: null }));
  }
  selectShortcut(id: string, version: number, enabled: boolean) {
    return this.change(async (preferences) => {
      if (enabled) await this.checkedItem(id, version, preferences.owner);
      const shortcutItems = preferences.shortcutItems.filter((value) => value !== id);
      if (enabled) shortcutItems.push(id);
      if (shortcutItems.length > 20)
        throw new Error(t('nativeSurfaces.removeOneOfThe20MiniappsFromShortcuts'));
      await this.save({ ...preferences, shortcutItems });
    });
  }
  startActivity(id: string, version: number, showContent: boolean) {
    return this.change(async (preferences) => {
      const item = await this.checkedItem(id, version, preferences.owner);
      const content = activityContent(item, preferences.owner, showContent);
      if (!content) throw new Error(t('nativeSurfaces.useACountdownThatEndsWithinTheNext'));
      for (const existing of this.port.activities()) await this.port.endActivity(existing);
      if (!this.current(preferences.owner))
        throw new Error(t('nativeSurfaces.theAccountHasChanged'));
      const activity = this.port.startActivity(content, preferences.owner, id);
      try {
        await this.save({
          ...preferences,
          activity: { ...activity, itemId: id, endsAt: content.endsAt, showContent },
        });
      } catch (error) {
        await this.port.endActivity(activity.id);
        throw error;
      }
    });
  }
  stopActivity() {
    return this.change(async (preferences) => {
      for (const id of this.port.activities()) await this.port.endActivity(id);
      await this.save({ ...preferences, activity: null });
    });
  }
}
