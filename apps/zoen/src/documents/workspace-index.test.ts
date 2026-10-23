import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ItemDetail } from '../core/types';
import {
  workspaceFolder,
  workspaceKind,
  workspaceRows,
  type WorkspaceFilter,
} from './workspace-index.ts';

function item(id: string, patch: Partial<ItemDetail> = {}): ItemDetail {
  return {
    id,
    title: id,
    space_id: 'work',
    space_title: 'Work',
    kind_id: 'page',
    path: '',
    text: null,
    app: null,
    file: null,
    versions: [],
    ...patch,
  } as ItemDetail;
}
const filter = (patch: Partial<WorkspaceFilter> = {}): WorkspaceFilter => ({
  spaces: new Set(['work', 'personal']),
  space: null,
  kind: 'all',
  query: '',
  folder: '',
  ...patch,
});
const ids = (rows: ReturnType<typeof workspaceRows>) =>
  rows.map((row) => (row.type === 'item' ? row.item.id : row.path));

test('revocation removes cached results and folders immediately, even on a failed/offline refresh', () => {
  const rows = [item('private', { space_id: 'personal', path: 'secret/a.md' }), item('shared')];
  assert.deepEqual(ids(workspaceRows(rows, filter({ spaces: new Set(['work']) }))), ['shared']);
  assert.deepEqual(
    workspaceRows(rows, filter({ spaces: new Set(['work']), space: 'personal' })),
    [],
  );
  assert.deepEqual(workspaceRows(rows, filter({ spaces: new Set() })), []);
});

test('nested folders are derived inside one chat without mixing identical paths from other chats', () => {
  const rows = [
    item('a', { path: 'plans/2026/a.md' }),
    item('b', { path: 'plans/b.md' }),
    item('outside', { space_id: 'personal', path: 'plans/c.md' }),
    item('root'),
  ];
  const root = workspaceRows(rows, filter({ space: 'work' }));
  assert.deepEqual(root[0], {
    type: 'folder',
    key: 'folder:plans',
    name: 'plans',
    path: 'plans',
    count: 2,
  });
  assert.deepEqual(ids(root), ['plans', 'root']);
  assert.deepEqual(ids(workspaceRows(rows, filter({ space: 'work', folder: 'plans' }))), [
    'plans/2026',
    'b',
  ]);
  assert.deepEqual(ids(workspaceRows(rows, filter({ space: 'work', folder: 'plan' }))), []);
});

test('search finds accented titles and page text across nested folders; every query term must match', () => {
  const rows = [
    item('a', { title: 'Apresentação', path: 'team/roadmap.md', text: 'Próximo lançamento' }),
    item('b', { title: 'Apresentação' }),
  ];
  assert.deepEqual(
    ids(workspaceRows(rows, filter({ space: 'work', query: 'apresentacao LANCAMENTO' }))),
    ['a'],
  );
  assert.deepEqual(ids(workspaceRows(rows, filter({ query: 'team roadmap' }))), ['a']);
  assert.deepEqual(workspaceRows(rows, filter({ query: 'unrelated' })), []);
});

test('recents use canonical history timestamps without losing 64-bit precision or mutating input', () => {
  const a = item('a', { versions: [{ at_ms: '9007199254740992' }] as ItemDetail['versions'] });
  const b = item('b', { versions: [{ at_ms: '9007199254740993' }] as ItemDetail['versions'] });
  const rows = [a, b];
  assert.deepEqual(ids(workspaceRows(rows, filter())), ['b', 'a']);
  assert.deepEqual(rows, [a, b]);
  assert.deepEqual(ids(workspaceRows([item('z'), item('a')], filter())), ['a', 'z']);
});

test('kind filters recognize real DeckCraft and image documents rather than translated labels', () => {
  const deck = item('deck', { kind_id: 'app', app: { app_id: 'deckcraft' } as ItemDetail['app'] });
  const photo = item('photo', {
    kind_id: 'file',
    file: { mime: 'image/jpeg' } as ItemDetail['file'],
  });
  const app = item('canvas', {
    kind_id: 'app',
    app: { app_id: 'excalidraw' } as ItemDetail['app'],
  });
  assert.equal(workspaceKind(deck), 'slides');
  assert.equal(workspaceKind(photo), 'images');
  assert.equal(workspaceKind(app), 'apps');
  assert.deepEqual(ids(workspaceRows([deck, photo, app], filter({ kind: 'slides' }))), ['deck']);
});

test('malformed paths cannot be normalized into misleading folders', () => {
  for (const path of ['../a.md', '/private/a.md', 'a//b.md', 'a/./b.md', 'a\\b/c.md'])
    assert.equal(workspaceFolder(item('a', { path })), '');
});
