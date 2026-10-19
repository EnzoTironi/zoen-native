import test from 'node:test';
import assert from 'node:assert/strict';
import { SurfaceCoordinator } from '../native-surfaces/coordinator.ts';
import { activityContent } from '../native-surfaces/activity.ts';
import { surfaceOwner, surfaceSpaces } from '../native-surfaces/access.ts';
import {
  surfaceActivityURL,
  surfaceItemURL,
  surfaceLinkPath,
  surfaceRouteInvocation,
} from '../native-surfaces/links.ts';
import { openSurfaceInvocation } from '../native-surfaces/intents.ts';
import type { SurfaceAccess } from '../native-surfaces/intents';
import type { MiniappActivityContent } from '../native-surfaces/activity';
import type { ItemDetail } from './generated/ItemDetail';
import type { Snapshot } from './generated/Snapshot';
import type { SurfacePort } from '../native-surfaces/coordinator';
import { privateWidgetTitle, widgetSelectionId } from '../native-surfaces/widget-catalog.ts';
import type { NativeWidgetCatalog } from '../native-surfaces/widget-catalog';
const owner = { identityId: 'id_a', deviceId: 'dev_a' };
function item(targetMs = Date.now() + 60_000, id = 'it_a'): ItemDetail {
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
test('surface links bind owner/device/version and reject duplicate or unsafe parameters', () => {
  const url = surfaceItemURL(owner, 'it_a', 3);
  assert.ok(surfaceLinkPath(url)?.startsWith('/surface?'));
  assert.equal(surfaceLinkPath(`${url}&version=3`), null);
  assert.equal(surfaceLinkPath(url.replace('version=3', 'version=9007199254740992')), null);
  assert.equal(surfaceLinkPath(url.replace('zoen://surface', 'https://surface')), null);
});
test('activities require a bounded canonical end and redact content by default', () => {
  const now = Date.now();
  assert.equal(activityContent(item(now - 1), owner, false, now), null);
  assert.equal(activityContent(item(now + 8 * 60 * 60_000 + 1), owner, false, now), null);
  const content = activityContent(item(now + 60_000), owner, false, now)!;
  assert.equal(content.redacted, true);
  assert.equal(JSON.stringify(content).includes('Private event'), false);
  assert.equal(content.endsAt, now + 60_000);
});
test('selection rereads canonical versions; lost access and account switches clear projections', async () => {
  let current = owner;
  let lost = false;
  let stored: string | null = null;
  const publications: unknown[] = [];
  const port: SurfacePort = {
    readPreferences: async () => stored,
    eraseMetadata: async () => {},
    writePreferences: async (value) => {
      stored = value;
    },
    widget: (value) => {
      publications.push(value);
    },
    catalogs: async (items, chats) => {
      publications.push({ items, chats });
    },
    activities: () => [],
    startActivity: () => ({ id: 'activity_a', linkId: 'link_a' }),
    updateActivity: async () => {},
    endActivity: async () => {},
  };
  const coordinator = new SurfaceCoordinator(
    {
      loading: () => false,
      owner: () => current,
      spaces: () => new Map([['sp_a', 'Private chat']]),
      item: async () => {
        if (lost) throw Error('unavailable');
        return item();
      },
      items: async () => {
        if (lost) throw Error('unavailable');
        return [item()];
      },
    },
    port,
  );
  coordinator.activate();
  await coordinator.refresh();
  await assert.rejects(coordinator.selectWidget('it_a', 2, true), /miniapp has changed/);
  assert.deepEqual(coordinator.getSnapshot().preferences?.widgets, []);
  await coordinator.selectWidget('it_a', 3, false);
  await coordinator.selectShortcut('it_a', 3, true);
  assert.equal(coordinator.getSnapshot().preferences?.widgets[0]?.showContent, false);
  lost = true;
  await coordinator.refresh();
  assert.deepEqual(publications.at(-1), { items: [], chats: [] });
  current = { ...owner, deviceId: 'dev_b' };
  await coordinator.refresh();
  assert.deepEqual(coordinator.getSnapshot().preferences?.shortcutItems, []);
  assert.deepEqual(coordinator.getSnapshot().preferences?.widgets, []);
  coordinator.dispose();
});

test('cleanup failures still clear independent surfaces and retire locked or switched account opt-ins', async () => {
  for (const failure of ['widget', 'catalogs', 'activities', 'activity_a', 'preferences']) {
    for (const switchAccount of [false, true]) {
      let current: typeof owner | null = owner;
      let failing = false;
      let stored: string | null = null;
      const attempted: string[] = [];
      const port: SurfacePort = {
        readPreferences: async () => stored,
        eraseMetadata: async () => {},
        writePreferences: async (value) => {
          if (value === null) {
            attempted.push('preferences');
            if (failing && failure === 'preferences') throw Error('storage unavailable');
          }
          stored = value;
        },
        widget: (content) => {
          if (content === null) {
            attempted.push('widget');
            if (failing && failure === 'widget') throw Error('widget unavailable');
          }
        },
        catalogs: async (items, chats) => {
          if (!items.length && !chats.length) {
            attempted.push('catalogs');
            if (failing && failure === 'catalogs') throw Error('intents unavailable');
          }
        },
        activities: () => {
          if (failing && failure === 'activities') throw Error('enumeration unavailable');
          return failing ? ['activity_a', 'activity_b'] : [];
        },
        startActivity: () => ({ id: 'activity_a', linkId: 'link_a' }),
        updateActivity: async () => {},
        endActivity: async (id) => {
          attempted.push(id);
          if (failing && failure === id) throw Error('activity unavailable');
        },
      };
      const coordinator = new SurfaceCoordinator(
        {
          loading: () => false,
          owner: () => current,
          spaces: () => new Map([['sp_a', 'Private chat']]),
          item: async () => item(),
          items: async () => [item()],
        },
        port,
      );
      coordinator.activate();
      await coordinator.refresh();
      await coordinator.selectWidget('it_a', 3, true);
      attempted.length = 0;
      current = switchAccount ? { ...owner, deviceId: 'dev_b' } : null;
      failing = true;
      await assert.doesNotReject(coordinator.refresh());
      for (const target of failure === 'activities'
        ? ['widget', 'catalogs', 'preferences']
        : ['widget', 'catalogs', 'activity_a', 'activity_b', 'preferences'])
        assert.ok(attempted.includes(target), `${failure} prevented cleanup of ${target}`);
      assert.equal(coordinator.getSnapshot().preferences, null);
      assert.equal(coordinator.getSnapshot().ready, false);
      assert.match(coordinator.getSnapshot().error ?? '', /surfaces could not be updated/);
      if (failure !== 'preferences') assert.equal(stored, null);
      failing = false;
      await coordinator.refresh();
      assert.equal(stored, null);
      assert.equal(coordinator.getSnapshot().error, null);
      assert.deepEqual(coordinator.getSnapshot().preferences?.widgets ?? [], []);
      if (switchAccount)
        assert.equal(coordinator.getSnapshot().preferences?.owner.deviceId, 'dev_b');
      coordinator.dispose();
    }
  }
});

test('explicit device erase waits for old publications and removes pending native metadata before reporting completion', async () => {
  let current: typeof owner | null = owner;
  let blockRead = false;
  let finishRead!: () => void;
  const read = new Promise<void>((resolve) => {
    finishRead = resolve;
  });
  const publications: (string | null)[] = [];
  let metadataAttempts = 0;
  let brokenMetadata = false;
  const coordinator = new SurfaceCoordinator(
    {
      loading: () => false,
      owner: () => current,
      spaces: () => new Map([['sp_a', 'Private chat']]),
      item: async () => item(),
      items: async () => {
        if (blockRead) await read;
        return [item()];
      },
    },
    {
      readPreferences: async () => null,
      writePreferences: async () => {},
      widget: (value) => {
        publications.push(value?.entries[0]?.content.title ?? null);
      },
      catalogs: async () => {},
      activities: () => [],
      startActivity: () => ({ id: 'activity_a', linkId: 'link_a' }),
      updateActivity: async () => {},
      endActivity: async () => {},
      eraseMetadata: async () => {
        metadataAttempts++;
        if (brokenMetadata) throw Error('intent storage unavailable');
      },
    },
  );
  coordinator.activate();
  await coordinator.refresh();
  await coordinator.selectWidget('it_a', 3, true);
  publications.length = 0;
  blockRead = true;
  const oldRefresh = coordinator.refresh();
  await Promise.resolve();
  await Promise.resolve();
  current = null;
  brokenMetadata = true;
  const erasing = coordinator.eraseDevice();
  const refusal = assert.rejects(erasing, /previews could not be erased/);
  finishRead();
  await oldRefresh;
  await refusal;
  assert.ok(publications.every((value) => value === null));
  assert.equal(coordinator.getSnapshot().preferences, null);
  assert.equal(metadataAttempts, 1);
  brokenMetadata = false;
  await coordinator.eraseDevice();
  assert.equal(metadataAttempts, 2);
  coordinator.dispose();
});

function activityInvocation(linkId: string, itemId = 'it_a') {
  const path = surfaceLinkPath(surfaceActivityURL(owner, itemId, linkId));
  assert.ok(path);
  return surfaceRouteInvocation(Object.fromEntries(new URLSearchParams(path.split('?')[1])));
}

function activityFixture(
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

test('an activity session opens the current version after refresh while its old widget link stays stale', async (t) => {
  const fixture = activityFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  const activity = await fixture.start();
  const invocation = activityInvocation(activity.linkId);
  assert.deepEqual(fixture.native.started, [
    { id: activity.id, linkId: activity.linkId, owner, itemId: 'it_a' },
  ]);

  fixture.environment.item = { ...fixture.environment.item, version: 4 };
  await fixture.coordinator.refresh();
  assert.equal(fixture.native.started.length, 1);
  assert.deepEqual(fixture.native.ended, []);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.activity?.linkId, activity.linkId);
  assert.equal(
    fixture.native.updated.at(-1)?.content.deepLink,
    surfaceActivityURL(owner, 'it_a', activity.linkId),
  );
  assert.deepEqual(await openSurfaceInvocation(invocation, fixture.access), {
    status: 'opened',
    path: '/item/it_a',
  });

  const widgetPath = surfaceLinkPath(surfaceItemURL(owner, 'it_a', 3));
  assert.ok(widgetPath);
  const widgetInvocation = surfaceRouteInvocation(
    Object.fromEntries(new URLSearchParams(widgetPath.split('?')[1])),
  );
  assert.deepEqual(await openSurfaceInvocation(widgetInvocation, fixture.access), {
    status: 'stale',
  });
  assert.deepEqual(fixture.opened, ['/item/it_a']);
});

test('activity admission denies retired, mismatched, inaccessible and inactive sessions', async (t) => {
  for (const scenario of [
    'stopped',
    'replaced',
    'native-gone',
    'lost-membership',
    'owner-changed',
    'locked',
    'changed-end',
    'different-item',
    'different-link',
    'loading',
    'disposed',
  ]) {
    await t.test(scenario, async (context) => {
      const fixture = activityFixture();
      context.after(() => fixture.coordinator.dispose());
      await fixture.ready();
      const activity = await fixture.start();
      let itemId = 'it_a';
      let linkId = activity.linkId;
      if (scenario === 'stopped') await fixture.coordinator.stopActivity();
      if (scenario === 'replaced') await fixture.start();
      if (scenario === 'native-gone') fixture.native.running.clear();
      if (scenario === 'lost-membership') fixture.environment.member = false;
      if (scenario === 'owner-changed') fixture.environment.owner = { ...owner, deviceId: 'dev_b' };
      if (scenario === 'locked') fixture.environment.owner = null;
      if (scenario === 'changed-end') fixture.environment.item = item(activity.endsAt + 60_000);
      if (scenario === 'different-item') {
        itemId = 'it_b';
        fixture.environment.item = item(activity.endsAt, itemId);
      }
      if (scenario === 'different-link') linkId = 'other_link';
      if (scenario === 'loading') fixture.environment.loading = true;
      if (scenario === 'disposed') fixture.coordinator.dispose();

      assert.equal(fixture.coordinator.canOpenActivity(fixture.environment.item, linkId), false);
      const result = await openSurfaceInvocation(
        activityInvocation(linkId, itemId),
        fixture.access,
      );
      assert.equal(
        result.status,
        ['owner-changed', 'locked'].includes(scenario) ? 'owner_changed' : 'stale',
      );
      assert.deepEqual(fixture.opened, []);
      if (scenario === 'replaced') {
        const replacement = fixture.coordinator.getSnapshot().preferences?.activity;
        assert.ok(replacement);
        assert.notEqual(replacement.linkId, activity.linkId);
        assert.equal(
          fixture.coordinator.canOpenActivity(fixture.environment.item, replacement.linkId),
          true,
        );
      }
    });
  }
});

test('an expired canonical countdown cannot open even while its native activity is still present', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 2_000_000 });
  const fixture = activityFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  const activity = await fixture.start();
  t.mock.timers.setTime(activity.endsAt + 1);
  assert.equal(fixture.native.running.has(activity.id), true);
  assert.equal(
    fixture.coordinator.canOpenActivity(fixture.environment.item, activity.linkId),
    false,
  );
  assert.deepEqual(
    await openSurfaceInvocation(activityInvocation(activity.linkId), fixture.access),
    { status: 'stale' },
  );
  assert.deepEqual(fixture.opened, []);
});

test('production membership helpers deny a cached conversation immediately and retire its selected activity on refresh', async (t) => {
  const session: Parameters<typeof surfaceOwner>[0] = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: owner.identityId,
        device_id: owner.deviceId,
        unlocked: true,
        setup_pending: false,
      },
      me: { id: owner.identityId },
      conversations: [
        {
          id: 'sp_a',
          title: 'Private chat',
          members: [
            { id: owner.identityId, is_me: false },
            { id: 'other_identity', is_me: true },
          ],
        },
      ],
    } as Snapshot,
  };
  const fixture = activityFixture(null, [], session);
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  const activity = await fixture.start();
  assert.equal(
    fixture.coordinator.canOpenActivity(fixture.environment.item, activity.linkId),
    true,
  );

  const cachedConversation = session.snapshot!.conversations[0]!;
  cachedConversation.members = cachedConversation.members.filter(
    (member) => member.id !== owner.identityId,
  );
  assert.equal(session.snapshot!.conversations[0], cachedConversation);
  assert.equal(fixture.native.running.has(activity.id), true);
  assert.equal(
    fixture.coordinator.canOpenActivity(fixture.environment.item, activity.linkId),
    false,
  );
  assert.deepEqual(
    await openSurfaceInvocation(activityInvocation(activity.linkId), fixture.access),
    { status: 'stale' },
  );
  assert.deepEqual(fixture.opened, []);

  await fixture.coordinator.refresh();
  assert.deepEqual(fixture.native.ended, [activity.id]);
  assert.equal(fixture.native.running.has(activity.id), false);
  assert.equal(fixture.native.started.length, 1);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.activity, null);
});

test('a cold coordinator restores its activity session before admitting a tap', async (t) => {
  const endsAt = Date.now() + 60_000;
  const fixture = activityFixture(
    JSON.stringify({
      schemaVersion: 2,
      owner,
      widgets: [],
      shortcutItems: [],
      activity: {
        id: 'activity_saved',
        linkId: 'saved_link',
        itemId: 'it_a',
        endsAt,
        showContent: false,
      },
    }),
    ['activity_saved'],
  );
  t.after(() => fixture.coordinator.dispose());
  fixture.environment.item = item(endsAt);
  assert.equal(fixture.coordinator.canOpenActivity(fixture.environment.item, 'saved_link'), false);
  await fixture.ready();
  assert.deepEqual(await openSurfaceInvocation(activityInvocation('saved_link'), fixture.access), {
    status: 'opened',
    path: '/item/it_a',
  });
  assert.deepEqual(fixture.native.started, []);
  assert.deepEqual(fixture.native.ended, []);
});

test('unsupported schema-one preferences retire the entire old selection and native activity', async (t) => {
  const fixture = activityFixture(
    JSON.stringify({
      schemaVersion: 1,
      owner,
      widget: { itemId: 'it_a', showContent: true },
      shortcutItems: ['it_a'],
      activity: {
        id: 'activity_old',
        itemId: 'it_a',
        endsAt: Date.now() + 60_000,
        showContent: false,
      },
    }),
    ['activity_old'],
  );
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  const preferences = fixture.coordinator.getSnapshot().preferences;
  assert.equal(preferences?.schemaVersion, 2);
  assert.deepEqual(preferences?.widgets, []);
  assert.deepEqual(preferences?.shortcutItems, []);
  assert.equal(preferences?.activity, null);
  assert.deepEqual(fixture.native.ended, ['activity_old']);
  assert.deepEqual(fixture.native.started, []);
});

test('a pending membership check cannot open an activity that was stopped or replaced meanwhile', async (t) => {
  for (const replace of [false, true]) {
    await t.test(replace ? 'replaced' : 'stopped', async (context) => {
      const fixture = activityFixture();
      context.after(() => fixture.coordinator.dispose());
      await fixture.ready();
      const activity = await fixture.start();
      let finishMembership!: (allowed: boolean) => void;
      const membership = new Promise<boolean>((resolve) => {
        finishMembership = resolve;
      });
      let didCheckMembership!: () => void;
      const checkingMembership = new Promise<void>((resolve) => {
        didCheckMembership = resolve;
      });
      const opening = openSurfaceInvocation(activityInvocation(activity.linkId), {
        ...fixture.access,
        canOpenSpace: () => {
          didCheckMembership();
          return membership;
        },
      });
      await checkingMembership;
      if (replace) await fixture.start();
      else await fixture.coordinator.stopActivity();
      finishMembership(true);
      assert.deepEqual(await opening, { status: 'stale' });
      assert.deepEqual(fixture.opened, []);
    });
  }
});

function widgetFixture() {
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

test('independent widget selections update one privacy choice and follow current canonical versions', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, false);
  const initial = fixture.catalog();
  assert.deepEqual(
    initial.entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_a'), widgetSelectionId(owner, 'it_b')],
  );
  assert.equal(initial.entries[0]?.content.redacted, false);
  assert.equal(initial.entries[1]?.content.redacted, true);
  assert.equal(initial.entries[1]?.configurationTitle, privateWidgetTitle(2));
  assert.equal(JSON.stringify(initial.entries[1]).includes('Private event'), false);

  await fixture.coordinator.selectWidget('it_a', 3, false);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_a', showContent: false },
    { itemId: 'it_b', showContent: true },
  ]);
  const current = fixture.environment.items.get('it_b')!;
  fixture.environment.items.set('it_b', { ...current, version: 4 });
  await fixture.coordinator.refresh();
  const updated = fixture.catalog();
  assert.deepEqual(
    updated.entries.map((entry) => entry.selectionId),
    initial.entries.map((entry) => entry.selectionId),
  );
  assert.equal(updated.entries[0]?.content.redacted, true);
  assert.equal(updated.entries[1]?.content.redacted, false);
  assert.equal(updated.entries[1]?.content.deepLink, surfaceItemURL(owner, 'it_b', 4));
  await assert.rejects(fixture.coordinator.selectWidget('it_b', 3, false), /miniapp has changed/);
  assert.equal(fixture.catalog().entries[1]?.content.redacted, false);
});

test('removing one widget preserves every other selection, key, privacy choice and shortcut', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, false);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  await fixture.coordinator.selectShortcut('it_b', 3, true);
  const second = fixture.catalog().entries[1];
  await fixture.coordinator.removeWidget('it_a');
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_b', showContent: true },
  ]);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.shortcutItems, ['it_b']);
  assert.deepEqual(fixture.catalog().entries, [second]);
  await fixture.coordinator.removeWidget('it_missing');
  assert.deepEqual(fixture.catalog().entries, [second]);
  await fixture.coordinator.removeWidget('it_b');
  assert.deepEqual(fixture.catalog().entries, []);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.shortcutItems, ['it_b']);
});

test('deleted and inaccessible selected items disappear independently without selecting a fallback', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  fixture.environment.items.set('it_unselected', item(Date.now() + 60_000, 'it_unselected'));
  fixture.environment.items.delete('it_a');
  await fixture.coordinator.refresh();
  assert.deepEqual(
    fixture.catalog().entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_b')],
  );
  fixture.environment.spaces.delete('sp_b');
  await fixture.coordinator.refresh();
  assert.deepEqual(fixture.catalog().entries, []);
  assert.deepEqual(
    fixture.coordinator.getSnapshot().preferences?.widgets.map((selection) => selection.itemId),
    ['it_a', 'it_b'],
  );
  fixture.environment.items.set('it_a', { ...item(), version: 5 });
  await fixture.coordinator.refresh();
  assert.deepEqual(
    fixture.catalog().entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_a')],
  );
  assert.equal(fixture.catalog().entries[0]?.content.deepLink, surfaceItemURL(owner, 'it_a', 5));
});

test('failed multi-widget saves and removals retain the last committed catalog and can retry', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  const first = fixture.catalog();
  const stored = fixture.stored();
  fixture.environment.failWrite = true;
  await assert.rejects(fixture.coordinator.selectWidget('it_b', 3, false), /storage unavailable/);
  assert.equal(fixture.catalog(), first);
  assert.equal(fixture.stored(), stored);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_a', showContent: true },
  ]);
  fixture.environment.failWrite = false;
  await fixture.coordinator.selectWidget('it_b', 3, false);
  const both = fixture.catalog();
  fixture.environment.failWrite = true;
  await assert.rejects(fixture.coordinator.removeWidget('it_a'), /storage unavailable/);
  assert.equal(fixture.catalog(), both);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 2);
  fixture.environment.failWrite = false;
  await fixture.coordinator.removeWidget('it_a');
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, [
    { itemId: 'it_b', showContent: false },
  ]);
});

test('twenty widgets allow exact updates and removal while a twenty-first add never writes preferences', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  for (let index = 0; index < 21; index++)
    fixture.environment.items.set(`it_${index}`, item(Date.now() + 60_000, `it_${index}`));
  for (let index = 0; index < 20; index++)
    await fixture.coordinator.selectWidget(`it_${index}`, 3, false);
  const writes = fixture.native.writes.length;
  await assert.rejects(fixture.coordinator.selectWidget('it_20', 3, true), /20/);
  assert.equal(fixture.native.writes.length, writes);
  assert.equal(fixture.catalog().entries.length, 20);
  await fixture.coordinator.selectWidget('it_5', 3, true);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 20);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets[5], {
    itemId: 'it_5',
    showContent: true,
  });
  await fixture.coordinator.removeWidget('it_3');
  await fixture.coordinator.selectWidget('it_20', 3, true);
  assert.deepEqual(
    fixture.coordinator.getSnapshot().preferences?.widgets.map((selection) => selection.itemId),
    [
      ...Array.from({ length: 20 }, (_, index) => `it_${index}`).filter((id) => id !== 'it_3'),
      'it_20',
    ],
  );
});

test('a late item read cannot publish an old-owner widget catalog and old native links cannot open the new account', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  const oldLink = fixture.catalog().entries[0]!.content.deepLink!;
  const oldKey = fixture.catalog().entries[0]!.selectionId;
  let entered!: () => void;
  const reading = new Promise<void>((resolve) => {
    entered = resolve;
  });
  let finish!: () => void;
  const waiting = new Promise<void>((resolve) => {
    finish = resolve;
  });
  fixture.environment.beforeBatch = async () => {
    entered();
    await waiting;
  };
  const publishedBefore = fixture.native.widgets.length;
  const refresh = fixture.coordinator.refresh();
  await reading;
  fixture.environment.owner = { identityId: 'id_b', deviceId: 'dev_b' };
  finish();
  await refresh;
  fixture.environment.beforeBatch = null;
  await fixture.coordinator.refresh();
  assert.ok(
    fixture.native.widgets.slice(publishedBefore).every((catalog) => !catalog?.entries.length),
  );
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, []);
  await fixture.coordinator.selectWidget('it_a', 3, true);
  assert.notEqual(fixture.catalog().entries[0]!.selectionId, oldKey);

  const path = surfaceLinkPath(oldLink);
  assert.ok(path);
  const opened: string[] = [];
  const result = await openSurfaceInvocation(
    surfaceRouteInvocation(Object.fromEntries(new URLSearchParams(path.split('?')[1]))),
    {
      currentOwner: () => fixture.environment.owner,
      item: async () => fixture.environment.items.get('it_a')!,
      canOpenSpace: async (id) => fixture.environment.spaces.has(id),
      open: (route) => {
        opened.push(route);
      },
    },
  );
  assert.deepEqual(result, { status: 'owner_changed' });
  assert.deepEqual(opened, []);
});

test('one bounded owner-scoped batch refreshes all selected widgets and shortcuts without full-item reads', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, false);
  await fixture.coordinator.selectShortcut('it_a', 3, true);
  fixture.native.batches.length = 0;
  fixture.native.singleReads.length = 0;
  await fixture.coordinator.refresh();
  assert.deepEqual(fixture.native.singleReads, []);
  assert.equal(fixture.native.batches.length, 1);
  assert.deepEqual(fixture.native.batches[0], { ids: ['it_a', 'it_b'], owner });
  assert.equal(fixture.catalog().entries.length, 2);
});

test('membership is rechecked after a delayed canonical batch and denied items never reach native publication', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, true);
  let entered!: () => void;
  const reading = new Promise<void>((resolve) => {
    entered = resolve;
  });
  let finish!: () => void;
  const waiting = new Promise<void>((resolve) => {
    finish = resolve;
  });
  fixture.environment.beforeBatch = async () => {
    entered();
    await waiting;
  };
  const refresh = fixture.coordinator.refresh();
  await reading;
  // Replace rather than mutate the map so a pre-await captured access map would
  // still include B and fail this production-boundary regression.
  fixture.environment.spaces = new Map([['sp_a', 'Private chat A']]);
  finish();
  await refresh;
  assert.deepEqual(
    fixture.catalog().entries.map((entry) => entry.selectionId),
    [widgetSelectionId(owner, 'it_a')],
  );
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 2);
});

test('a failed canonical batch clears native projections without substituting individual or cached item reads', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  await fixture.coordinator.selectWidget('it_b', 3, false);
  fixture.native.batches.length = 0;
  fixture.native.singleReads.length = 0;
  fixture.environment.failBatch = true;
  await fixture.coordinator.refresh();
  assert.equal(fixture.native.widgets.at(-1), null);
  assert.equal(fixture.native.batches.length, 1);
  assert.deepEqual(fixture.native.singleReads, []);
  assert.match(fixture.coordinator.getSnapshot().error ?? '', /surfaces could not be updated/);
  assert.equal(fixture.coordinator.getSnapshot().preferences?.widgets.length, 2);
  fixture.environment.failBatch = false;
  await fixture.coordinator.refresh();
  assert.equal(fixture.catalog().entries.length, 2);
  assert.equal(fixture.coordinator.getSnapshot().error, null);
});

test('a delayed preference acknowledgment cannot publish the old owner choice into the next account', async (t) => {
  const fixture = widgetFixture();
  t.after(() => fixture.coordinator.dispose());
  await fixture.ready();
  await fixture.coordinator.selectWidget('it_a', 3, true);
  let entered!: () => void;
  const writing = new Promise<void>((resolve) => {
    entered = resolve;
  });
  let finish!: () => void;
  const waiting = new Promise<void>((resolve) => {
    finish = resolve;
  });
  fixture.environment.beforeWrite = async () => {
    entered();
    await waiting;
  };
  const baseline = fixture.native.widgets.length;
  const changing = fixture.coordinator.selectWidget('it_b', 3, true);
  const refused = assert.rejects(changing, /account has changed/);
  await writing;
  fixture.environment.owner = { identityId: 'identity_next', deviceId: 'device_next' };
  finish();
  await refused;
  fixture.environment.beforeWrite = null;
  await fixture.coordinator.refresh();
  assert.equal(fixture.stored(), null);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.widgets, []);
  assert.deepEqual(fixture.coordinator.getSnapshot().preferences?.owner, fixture.environment.owner);
  assert.ok(fixture.native.widgets.slice(baseline).every((catalog) => !catalog?.entries.length));
});
