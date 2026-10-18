import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ItemCard } from '../core/generated/ItemCard';
import type { ItemDetail } from '../core/generated/ItemDetail';
import type { AppStateDto } from '../core/generated/AppStateDto';
import { miniappMessageProjection } from './message-card-model.ts';

const app = (headline: string, id = 'poll'): AppStateDto => ({
  app_id: id,
  resource_uri: 'ui://poll/view',
  name: 'Poll',
  view_json: '{}',
  headline,
  metrics: [],
  last_action: null,
  trust: 'Suggest',
  snapshot_json: '',
});
function card(): ItemCard {
  return {
    item_id: 'it_poll',
    title: 'Original title',
    summary: 'Original summary',
    kind_label: 'Miniapp',
    kind_id: 'app',
    version: 2,
    total_cents: null,
    budget_cents: null,
    line_count: 0,
    done_count: 0,
    app: app('Embedded canonical state'),
  };
}
function live(): ItemDetail {
  return {
    id: 'it_poll',
    space_id: 'sp_chat',
    space_title: 'Chat',
    kind_label: 'Miniapp',
    kind_id: 'app',
    title: 'Current title',
    origin: 'Created',
    created_by: {} as ItemDetail['created_by'],
    plan: null,
    text: null,
    version: 3,
    versions: [],
    linked_requests: [],
    app: app('Current canonical state'),
    path: '',
    file: null,
  };
}

test('embedded signed message renders without an ItemDetail or a per-message fetch', () => {
  const source = card();
  const result = miniappMessageProjection(source, undefined, 'sp_chat')!;
  assert.deepEqual(Object.keys(result).sort(), ['app', 'id', 'space_id', 'title', 'version']);
  assert.equal(result.id, source.item_id);
  assert.equal(result.space_id, 'sp_chat');
  assert.strictEqual(result.app, source.app);
  assert.equal(result.version, 2);
  assert.equal(miniappMessageProjection(source), null, 'missing scope is never fabricated');
});

test('matching newer canonical item supplies live content; message inputs are untouched', () => {
  const source = card();
  const item = live();
  const result = miniappMessageProjection(source, item, 'sp_chat')!;
  assert.strictEqual(result.app, item.app);
  assert.equal(result.title, 'Current title');
  assert.equal(result.version, 3);
  assert.equal(source.title, 'Original title');
  assert.strictEqual(miniappMessageProjection(source, item)?.app, item.app);
});

test('foreign item, foreign scope, older version or different app cannot replace signed message state', () => {
  const source = card();
  for (const wrong of [
    { ...live(), id: 'it_other' },
    { ...live(), space_id: 'sp_other' },
    { ...live(), version: 1 },
    { ...live(), app: app('Different app', 'pet') },
    { ...live(), app: null },
    { ...live(), kind_id: 'note' },
  ]) {
    const result = miniappMessageProjection(source, wrong, 'sp_chat')!;
    assert.strictEqual(result.app, source.app);
    assert.equal(result.title, source.title);
    assert.equal(result.space_id, 'sp_chat');
    assert.equal(result.version, source.version);
  }
  assert.equal(miniappMessageProjection(source, { ...live(), id: 'it_other' }), null);
});

test('non-app message never adopts unrelated app content or invents a miniapp', () => {
  const source = card();
  assert.equal(miniappMessageProjection({ ...source, app: null }, live(), 'sp_chat'), null);
  assert.equal(miniappMessageProjection({ ...source, kind_id: 'note' }, live(), 'sp_chat'), null);
});
