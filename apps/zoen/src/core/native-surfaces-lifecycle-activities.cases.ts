import assert from 'node:assert/strict';
import test from 'node:test';
import type { surfaceOwner } from '../native-surfaces/access.ts';
import { activityContent } from '../native-surfaces/activity.ts';
import type { SurfacePort } from '../native-surfaces/coordinator';
import { SurfaceCoordinator } from '../native-surfaces/coordinator.ts';
import { openSurfaceInvocation } from '../native-surfaces/intents.ts';
import {
  surfaceActivityURL,
  surfaceItemURL,
  surfaceLinkPath,
  surfaceRouteInvocation,
} from '../native-surfaces/links.ts';
import type { Snapshot } from './generated/Snapshot';
import {
  activityFixture,
  activityInvocation,
  item,
  owner,
} from './native-surfaces-lifecycle.fixtures.ts';

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
  await assert.rejects(coordinator.selectWidget('it_a', 2, true), /app has changed/);
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
