import assert from 'node:assert/strict';
import { surfaceOwner, surfaceSpaces } from '../native-surfaces/access.ts';
import type { MiniappActivityContent } from '../native-surfaces/activity';
import { SurfaceCoordinator } from '../native-surfaces/coordinator.ts';
import type { SurfaceAccess } from '../native-surfaces/intents';
import {
  surfaceActivityURL,
  surfaceLinkPath,
  surfaceRouteInvocation,
} from '../native-surfaces/links.ts';
import type { NativeWidgetCatalog } from '../native-surfaces/widget-catalog';
import type { ItemDetail } from './generated/ItemDetail';

export const owner = { identityId: 'id_a', deviceId: 'dev_a' };
export function item(targetMs = Date.now() + 60_000, id = 'it_a'): ItemDetail {
  return {
    id,
    title: 'Private event',
    space_id: 'sp_a',
    version: 3,
    app: {
      app_id: 'countdown',
      snapshot_json: JSON.stringify({
        id,
        appId: 'countdown',
        deepLink: `zoen://app/${id}`,
        template: 'countdown',
        title: 'Private event',
        targetMs,
        accentHex: '#123456',
        symbol: 'timer',
      }),
    },
  } as ItemDetail;
}

export function activityInvocation(linkId: string, itemId = 'it_a') {
  const path = surfaceLinkPath(surfaceActivityURL(owner, itemId, linkId));
  assert.ok(path);
  return surfaceRouteInvocation(Object.fromEntries(new URLSearchParams(path.split('?')[1])));
}

export function activityFixture(
  stored: string | null = null,
  running: string[] = [],
  session?: Parameters<typeof surfaceOwner>[0],
) {
  const environment = {
    owner: owner as typeof owner | null,
    loading: false,
    member: true,
    item: item(),
  };
  const native = {
    running: new Set(running),
    started: [] as { id: string; linkId: string; owner: typeof owner; itemId: string }[],
    updated: [] as { id: string; content: MiniappActivityContent }[],
    ended: [] as string[],
  };
  const currentOwner = () => (session ? surfaceOwner(session) : environment.owner);
  const spaces = () =>
    session
      ? surfaceSpaces(session)
      : new Map(environment.member ? [['sp_a', 'Private chat']] : []);
  const coordinator = new SurfaceCoordinator(
    {
      loading: () => environment.loading,
      owner: currentOwner,
      spaces,
      item: async () => environment.item,
      items: async () => [environment.item],
    },
    {
      readPreferences: async () => stored,
      writePreferences: async (value) => {
        stored = value;
      },
      eraseMetadata: async () => {},
      widget: () => {},
      catalogs: async () => {},
      activities: () => [...native.running],
      startActivity: (_content, activityOwner, itemId) => {
        const suffix = native.started.length + 1;
        const activity = { id: `activity_${suffix}`, linkId: `link_${suffix}` };
        native.started.push({ ...activity, owner: activityOwner, itemId });
        native.running.add(activity.id);
        return activity;
      },
      updateActivity: async (id, content) => {
        native.updated.push({ id, content });
      },
      endActivity: async (id) => {
        native.ended.push(id);
        native.running.delete(id);
      },
    },
  );
  const opened: string[] = [];
  const access: SurfaceAccess = {
    currentOwner,
    item: async () => environment.item,
    canOpenSpace: async (spaceId) => spaces().has(spaceId),
    canOpenActivity: coordinator.canOpenActivity,
    open: (path) => {
      opened.push(path);
    },
  };
  return {
    coordinator,
    environment,
    native,
    opened,
    access,
    async ready() {
      coordinator.activate();
      await coordinator.refresh();
    },
    async start() {
      await coordinator.startActivity(environment.item.id, environment.item.version, false);
      const activity = coordinator.getSnapshot().preferences?.activity;
      assert.ok(activity);
      return activity;
    },
  };
}

export function widgetFixture() {
  const environment = {
    owner: owner as typeof owner | null,
    items: new Map([
      ['it_a', item()],
      ['it_b', { ...item(Date.now() + 60_000, 'it_b'), space_id: 'sp_b' }],
    ]),
    spaces: new Map([
      ['sp_a', 'Private chat A'],
      ['sp_b', 'Private chat B'],
    ]),
    beforeRead: null as ((id: string) => Promise<void>) | null,
    beforeBatch: null as ((ids: readonly string[]) => Promise<void>) | null,
    beforeWrite: null as ((value: string | null) => Promise<void>) | null,
    failWrite: false,
    failBatch: false,
  };
  let stored: string | null = null;
  const native = {
    widgets: [] as (NativeWidgetCatalog | null)[],
    writes: [] as (string | null)[],
    batches: [] as { ids: string[]; owner: typeof owner }[],
    singleReads: [] as string[],
  };
  const coordinator = new SurfaceCoordinator(
    {
      loading: () => false,
      owner: () => environment.owner,
      spaces: () => environment.spaces,
      item: async (id) => {
        native.singleReads.push(id);
        await environment.beforeRead?.(id);
        const current = environment.items.get(id);
        if (!current) throw Error('Item no longer exists');
        return current;
      },
      items: async (ids, currentOwner) => {
        native.batches.push({ ids: [...ids], owner: { ...currentOwner } });
        await environment.beforeBatch?.(ids);
        if (environment.failBatch) throw Error('Canonical batch unavailable');
        return ids.flatMap((id) => environment.items.get(id) ?? []);
      },
    },
    {
      readPreferences: async () => stored,
      writePreferences: async (value) => {
        native.writes.push(value);
        await environment.beforeWrite?.(value);
        if (environment.failWrite) throw Error('Preference storage unavailable');
        stored = value;
      },
      eraseMetadata: async () => {},
      widget: (catalog) => native.widgets.push(catalog),
      catalogs: async () => {},
      activities: () => [],
      startActivity: () => ({ id: 'activity_a', linkId: 'link_a' }),
      updateActivity: async () => {},
      endActivity: async () => {},
    },
  );
  return {
    coordinator,
    environment,
    native,
    stored: () => stored,
    async ready() {
      coordinator.activate();
      await coordinator.refresh();
    },
    catalog() {
      const catalog = native.widgets.at(-1);
      assert.ok(catalog);
      return catalog;
    },
  };
}
