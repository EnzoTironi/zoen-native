import test from 'node:test';
import assert from 'node:assert/strict';
import { SurfaceCoordinator } from '../native-surfaces/coordinator.ts';
import { activityContent } from '../native-surfaces/activity.ts';
import { surfaceItemURL, surfaceLinkPath } from '../native-surfaces/links.ts';
import type { ItemDetail } from './generated/ItemDetail';
import type { SurfacePort } from '../native-surfaces/coordinator';
const owner = { identityId: 'id_a', deviceId: 'dev_a' };
function item(targetMs = Date.now() + 60_000): ItemDetail {
  return {
    id: 'it_a',
    title: 'Private event',
    space_id: 'sp_a',
    version: 3,
    app: {
      app_id: 'countdown',
      snapshot_json: JSON.stringify({
        id: 'it_a',
        appId: 'countdown',
        deepLink: 'zoen://app/it_a',
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
    startActivity: () => 'activity_a',
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
    },
    port,
  );
  coordinator.activate();
  await coordinator.refresh();
  await assert.rejects(coordinator.selectWidget('it_a', 2, true), /mudou/);
  assert.equal(coordinator.getSnapshot().preferences?.widget, null);
  await coordinator.selectWidget('it_a', 3, false);
  await coordinator.selectShortcut('it_a', 3, true);
  assert.equal(coordinator.getSnapshot().preferences?.widget?.showContent, false);
  lost = true;
  await coordinator.refresh();
  assert.deepEqual(publications.at(-1), { items: [], chats: [] });
  current = { ...owner, deviceId: 'dev_b' };
  await coordinator.refresh();
  assert.deepEqual(coordinator.getSnapshot().preferences?.shortcutItems, []);
  assert.equal(coordinator.getSnapshot().preferences?.widget, null);
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
        startActivity: () => 'activity_a',
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
      assert.match(coordinator.getSnapshot().error ?? '', /atualizar/);
      if (failure !== 'preferences') assert.equal(stored, null);
      failing = false;
      await coordinator.refresh();
      assert.equal(stored, null);
      assert.equal(coordinator.getSnapshot().error, null);
      assert.equal(coordinator.getSnapshot().preferences?.widget ?? null, null);
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
      item: async () => {
        if (blockRead) await read;
        return item();
      },
    },
    {
      readPreferences: async () => null,
      writePreferences: async () => {},
      widget: (value) => {
        publications.push(value?.title ?? null);
      },
      catalogs: async () => {},
      activities: () => [],
      startActivity: () => 'activity_a',
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
  const refusal = assert.rejects(erasing, /prévias/);
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
