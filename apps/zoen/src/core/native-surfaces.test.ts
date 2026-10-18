import assert from 'node:assert/strict';
import { test } from 'node:test';
import { itemEntityCatalog } from '../native-surfaces/catalog.ts';
import { openSurfaceInvocation, surfaceNavigation } from '../native-surfaces/intents.ts';
import type { SurfaceAccess, SurfaceInvocation } from '../native-surfaces/intents.ts';
import { miniappSnapshot, widgetProjection } from '../native-surfaces/snapshot.ts';
import type { SnapshotItem } from '../native-surfaces/snapshot.ts';

const owner = { identityId: 'identity_1', deviceId: 'device_1' };
const snapshot = {
  id: 'it_pet',
  appId: 'pet',
  template: 'progress',
  title: 'Peanut',
  detail: 'Feliz aqui',
  bars: [{ label: 'Comida', value: 0.7 }],
  accentHex: '#E8833A',
  symbol: 'pawprint.fill',
  art: 'pet',
  actions: [{ tool: 'pet_feed', label: 'Comer' }],
  deepLink: 'zoen://app/it_pet',
};
function item(changes: Record<string, unknown> = {}): SnapshotItem {
  return {
    id: 'it_pet',
    space_id: 'sp_family',
    version: 3,
    app: {
      app_id: 'pet',
      resource_uri: 'zoen://apps/pet',
      name: 'Pet',
      view_json: '{"private":"not for extension"}',
      headline: 'Private headline',
      metrics: [],
      last_action: 'Private action',
      trust: 'Suggest',
      snapshot_json: JSON.stringify({ ...snapshot, ...changes }),
    },
  };
}
const now = 1_800_000_000_000;
function invocation(changes: Partial<SurfaceInvocation> = {}): SurfaceInvocation {
  return {
    id: 'invocation_1',
    name: 'zoen.openItem',
    params: { ...owner, itemId: 'it_pet', version: 3 },
    createdAt: now,
    ...changes,
  };
}
function access(changes: Partial<SurfaceAccess> = {}) {
  const opened: string[] = [];
  const api: SurfaceAccess = {
    currentOwner: () => owner,
    item: async () => item(),
    canOpenSpace: async (id) => id === 'sp_family',
    open: (path) => {
      opened.push(path);
    },
    ...changes,
  };
  return { api, opened };
}

test('widget content binds the signed item/app/deep link and rejects remote or invalid templates', () => {
  assert.equal(miniappSnapshot(item())?.title, 'Peanut');
  for (const changes of [
    { id: 'it_other' },
    { appId: 'poll' },
    { deepLink: 'zoen://app/it_other' },
    { deepLink: 'zoen://app/it_pet?execute=pet_feed' },
    { title: 'https://remote.test/private' },
    { template: 'webview' },
    { template: 'countdown', targetMs: 1e30 },
    { template: 'photo', photo: 'https://remote.test/photo' },
    { template: 'ticket', codes: ['ONLY'] },
    { symbol: 'pawprint.fill;execute' },
  ])
    assert.equal(miniappSnapshot(item(changes)), null, JSON.stringify(changes));
  assert.equal(miniappSnapshot({ ...item(), version: NaN }), null);
  assert.equal(
    miniappSnapshot({ ...item(), app: { ...item().app!, snapshot_json: 'x'.repeat(16_385) } }),
    null,
  );
});

test('bounded snapshot projection preserves canonical content without interpreting actions or app state', () => {
  const projected = miniappSnapshot(
    item({
      bars: [
        { label: 'Value', value: 9 },
        { label: 'Low', value: -9 },
        { label: 'Third', value: 0.2 },
        { label: 'Extra', value: 1 },
      ],
      rows: [
        { text: 'First', done: false },
        { text: 'file:///private', done: true },
      ],
      detail: 'Partida: 08h',
      targetMs: now,
      art: 'remote-avatar',
    }),
  );
  assert.deepEqual(
    projected?.bars.map((bar) => bar.value),
    [1, 0, 0.2],
  );
  assert.deepEqual(projected?.rows, [{ text: 'First', done: false }]);
  assert.equal(projected?.detail, 'Partida: 08h');
  assert.equal(projected?.art, undefined);
  assert.ok(Object.isFrozen(projected));
  const visible = widgetProjection(item(), owner, true)!;
  assert.equal(visible.version, 3);
  assert.equal(visible.content.title, 'Peanut');
  assert.equal('actions' in visible.content, false);
  assert.equal('view_json' in visible.content, false);
  assert.equal(JSON.stringify(visible).includes('not for extension'), false);
  assert.equal(JSON.stringify(visible).includes('Private headline'), false);
});

test('widget content is redacted until explicitly selected and owns a copied account/device scope', () => {
  const mutableOwner = { ...owner };
  const redacted = widgetProjection(item(), mutableOwner)!;
  mutableOwner.identityId = 'other';
  assert.deepEqual(redacted.owner, owner);
  assert.equal(redacted.content.redacted, true);
  assert.equal(redacted.content.title, 'Zoen');
  assert.deepEqual(redacted.content.bars, []);
  assert.equal(JSON.stringify(redacted).includes('Peanut'), false);
  assert.equal(JSON.stringify(redacted).includes('pet_feed'), false);
  assert.equal(widgetProjection(item(), { ...owner, deviceId: '../other' }), null);
});

test('entity catalogs publish only explicitly selected accessible IDs, not messages or previews', () => {
  const current = { id: 'it_pet', space_id: 'sp_family', title: 'Peanut', version: 3 };
  assert.deepEqual(itemEntityCatalog([current], owner, new Set(), new Set(['sp_family'])), []);
  assert.deepEqual(itemEntityCatalog([current], owner, new Set(['it_pet']), new Set()), []);
  const catalog = itemEntityCatalog(
    [current, current],
    owner,
    new Set(['it_pet']),
    new Set(['sp_family']),
  );
  assert.equal(catalog.length, 1);
  assert.equal(catalog[0]?.hideInSpotlight, true);
  assert.deepEqual(catalog[0]?.metadata, { ...owner, version: '3' });
  assert.deepEqual(Object.keys(catalog[0]!).sort(), ['hideInSpotlight', 'id', 'metadata', 'title']);
});

test('surface invocations reject mutation names, foreign owner, stale versions and lost access', async () => {
  const unsupported = access();
  assert.deepEqual(
    await openSurfaceInvocation(invocation({ name: 'send_message' }), unsupported.api, now),
    { status: 'unsupported' },
  );
  assert.equal(
    surfaceNavigation(
      invocation({ params: { ...owner, itemId: 'it_pet', version: 3, execute: true } }),
      now,
    ),
    null,
  );
  assert.equal(surfaceNavigation(invocation({ createdAt: now - 86_400_001 }), now), null);
  const foreign = access({ currentOwner: () => ({ ...owner, deviceId: 'other' }) });
  assert.deepEqual(await openSurfaceInvocation(invocation(), foreign.api, now), {
    status: 'owner_changed',
  });
  const stale = access({ item: async () => ({ ...item(), version: 4 }) });
  assert.deepEqual(await openSurfaceInvocation(invocation(), stale.api, now), { status: 'stale' });
  const denied = access({ canOpenSpace: async () => false });
  assert.deepEqual(await openSurfaceInvocation(invocation(), denied.api, now), {
    status: 'unavailable',
  });
  for (const result of [unsupported, foreign, stale, denied]) assert.deepEqual(result.opened, []);
});

test('item/chat navigation can replay safely but a mid-read account switch never opens a route', async () => {
  const current = access();
  assert.deepEqual(await openSurfaceInvocation(invocation(), current.api, now), {
    status: 'opened',
    path: '/item/it_pet',
  });
  assert.deepEqual(await openSurfaceInvocation(invocation(), current.api, now), {
    status: 'opened',
    path: '/item/it_pet',
  });
  assert.deepEqual(
    await openSurfaceInvocation(
      invocation({ name: 'zoen.openChat', params: { ...owner, spaceId: 'sp_family' } }),
      current.api,
      now,
    ),
    { status: 'opened', path: '/chat/sp_family' },
  );
  let active = true;
  const switched = access({
    currentOwner: () => (active ? owner : null),
    canOpenSpace: async () => {
      active = false;
      return true;
    },
  });
  assert.deepEqual(await openSurfaceInvocation(invocation(), switched.api, now), {
    status: 'owner_changed',
  });
  assert.deepEqual(switched.opened, []);
});
