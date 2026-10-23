import assert from 'node:assert/strict';
import test from 'node:test';
import type { ItemDetail, PageDto } from '../core/types';
import { pageImagePath, pageImageReference } from './page-image-reference.ts';
const hash = 'ab'.repeat(32);
const page = { space_id: 'work', path: 'notes/page.md' };
const image = (patch: Partial<ItemDetail> = {}) =>
  ({
    id: 'photo',
    space_id: 'work',
    path: 'notes/images/sky.png',
    kind_id: 'file',
    version: 4,
    file: { mime: 'image/png', sha256: hash },
    ...patch,
  }) as ItemDetail;

test('relative image paths normalize inside the same Space without becoming local SDK paths', () => {
  assert.equal(pageImagePath('notes/page.md', './images/sky.png'), 'notes/images/sky.png');
  assert.equal(pageImagePath('notes/page.md', '../images/sky.png'), 'images/sky.png');
  assert.equal(pageImagePath('', 'images/sky.png'), 'images/sky.png');
  assert.equal(
    pageImagePath('notes/page.md', 'images/c%C3%A9u%20azul.png'),
    'notes/images/céu azul.png',
  );
  assert.equal(pageImagePath('notas/página.md', 'ceu\u0301.png'), 'notas/ceú.png');
  for (const source of [
    '../../other.png',
    '///host/file',
    '/tmp/file',
    'file:///secret.png',
    'data:image/png;base64,YQ==',
    'blob:https://host/x',
    'zoen:secret',
    'https://host/image.png',
    'https://user:secret@host/photo.png',
    'C:\\secret.png',
    '..\\photo.png',
    '%2Fetc/passwd',
    '%5cphoto.png',
    '%2566ile.png',
    'image.png?token=private',
    'image.png#other',
    'image.png%3Ftoken=private',
    'file%3Asecret',
    'image\u0000.png',
    '%ZZ.png',
    'images//sky.png',
  ])
    assert.equal(pageImagePath('notes/page.md', source), null, source);
  for (const parent of [
    '/tmp/page.md',
    '../page.md',
    'notes//page.md',
    'notes/../page.md',
    'file:page.md',
    'notes\\page.md',
  ])
    assert.equal(pageImagePath(parent, 'sky.png'), null, parent);
});

test('exact unique canonical File identity, observed version and hash reach the loader', () => {
  const file = image();
  assert.deepEqual(pageImageReference(page, 'images/sky.png', [file]), {
    itemId: 'photo',
    version: 4,
    sha256: hash,
  });
  assert.equal(pageImageReference(page, 'images/sky.png', [image({ space_id: 'other' })]), null);
  assert.equal(
    pageImageReference(page, 'images/sky.png', [file, image({ id: 'duplicate' })]),
    null,
  );
  assert.equal(
    pageImageReference(page, 'images/sky.png', [
      file,
      image({ id: 'other-space', space_id: 'other' }),
    ])?.itemId,
    'photo',
  );
  assert.equal(
    pageImageReference(page, 'images/sky.png', [image({ path: 'notes/images/SKY.png' })]),
    null,
  );
  assert.equal(pageImageReference(page, 'images/sky.png', []), null);
  assert.equal(pageImageReference(page, 'https://host/image.png', [file]), null);
});

test('a replaced or unavailable Item cannot silently substitute another content type or invalid version/hash', () => {
  const original = image();
  for (const file of [
    image({ kind_id: 'page' }),
    image({ file: null }),
    image({ version: 0 }),
    image({ version: NaN }),
    image({ version: 1.5 }),
    image({ file: { ...original.file!, mime: 'text/html' } }),
    image({ file: { ...original.file!, mime: 'image/svg+xml' } }),
    image({ file: { ...original.file!, sha256: '' } }),
    image({ file: { ...original.file!, sha256: 'z'.repeat(64) } }),
  ])
    assert.equal(pageImageReference(page, 'images/sky.png', [file]), null);
  assert.deepEqual(
    pageImageReference(page, 'images/sky.png', [
      image({ version: 5, file: { ...original.file!, sha256: 'cd'.repeat(32) } }),
    ]),
    { itemId: 'photo', version: 5, sha256: 'cd'.repeat(32) },
  );
});

test('a historical Page resolves its own relative path, never the current Page directory', () => {
  const old = { ...page, path: 'archive/page.md', version: 1 } as PageDto;
  const items = [
    image(),
    image({ id: 'archived-photo', path: 'archive/images/sky.png', version: 2 }),
  ];
  assert.equal(pageImageReference(page, 'images/sky.png', items)?.itemId, 'photo');
  assert.deepEqual(pageImageReference(old, 'images/sky.png', items), {
    itemId: 'archived-photo',
    version: 2,
    sha256: hash,
  });
});
