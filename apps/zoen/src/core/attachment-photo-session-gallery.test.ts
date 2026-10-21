import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentPhotoSession, type PhotoPickerPort } from '../media/attachment-photo-session.ts';
import type { AttachmentAsset } from '../media/attachment-draft';
import type {
  PhotoLibraryAlbum,
  PhotoLibraryItem,
  PhotoLibraryPermission,
} from '../media/attachment-photo-library';

const item = (id: string): PhotoLibraryItem => ({
  id: `ph://${id}`,
  filename: `${id}.jpg`,
  mediaType: 'image' as PhotoLibraryItem['mediaType'],
  width: 100,
  height: 200,
  duration: null,
  creationTime: null,
  modificationTime: null,
  isFavorite: false,
});
const file = (id: string): AttachmentAsset => ({
  uri: `file:///owned/${id.slice(5)}`,
  mime: 'image/jpeg',
  name: `${id.slice(5)}.jpg`,
  size: 12,
});
const tick = async () => {
  for (let index = 0; index < 14; index++) await Promise.resolve();
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function fixture() {
  let current = true;
  const copied: string[][] = [],
    discarded: string[] = [];
  const albums: PhotoLibraryAlbum[] = [
    { id: 'first', title: 'First album' },
    { id: 'second', title: 'Second album' },
  ];
  const port: PhotoPickerPort = {
    permission: async () => ({
      status: 'granted' as PhotoLibraryPermission['status'],
      granted: true,
      canAskAgain: true,
      accessPrivileges: 'all',
    }),
    requestPermission: async () => {
      throw new Error('Already granted');
    },
    albums: async () => albums,
    load: async (id) =>
      id === 'second' ? [item('d'), item('e')] : [item('a'), item('b'), item('c')],
    refresh: async (items) => [...items],
    albumDetails: async () => ({ count: 3, previewId: 'ph://a' }),
    manage: async () => {},
    materialize: async (items) => {
      copied.push(items.map((value) => value.id));
      return items.map((value) => file(value.id));
    },
    capture: async () => [],
    discard: (uri) => {
      discarded.push(uri);
    },
  };
  const session = new AttachmentPhotoSession(port);
  return {
    session,
    port,
    copied,
    discarded,
    current: () => current,
    retire: () => {
      current = false;
      session.cancel();
    },
  };
}

test('gallery preview and back preserve the Photos request, selection and album without copying', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.album('second');
  await tick();
  const token = f.session.getSnapshot().token;
  f.session.select(item('e'));
  f.session.preview('ph://d');
  assert.equal(f.session.getSnapshot().screen, 'preview');
  assert.equal(f.session.getSnapshot().previewId, 'ph://d');
  assert.equal(f.session.getSnapshot().albumId, 'second');
  assert.equal(f.session.getSnapshot().token, token);
  assert.equal(f.session.getSnapshot().visible, true);
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://e'],
  );
  assert.deepEqual(f.copied, []);
  f.session.showGrid();
  assert.equal(f.session.getSnapshot().screen, 'grid');
  assert.equal(f.session.getSnapshot().previewId, null);
  assert.equal(f.session.getSnapshot().albumId, 'second');
  f.session.showAlbums();
  assert.equal(f.session.getSnapshot().screen, 'albums');
  f.session.album('first');
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://e'],
  );
  f.session.preview('ph://e');
  assert.equal(
    f.session.getSnapshot().previewId,
    'ph://e',
    'A retained selection remains viewable across albums',
  );
  f.session.cancel();
  assert.deepEqual(await result, []);
});

test('gallery current item is selected only by approve and materialized after the prior selection in order', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.preview('ph://a');
  assert.equal(f.session.canApprove('ph://a'), true);
  assert.equal(f.session.canApprove('ph://foreign'), false);
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  await f.session.approve('ph://a');
  assert.deepEqual(f.copied, [['ph://b', 'ph://a']]);
  assert.equal(f.session.getSnapshot().visible, false);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(await result, [file('ph://b'), file('ph://a')]);
});

test('zero-selection gallery can approve its current known item while an empty grid and foreign items cannot', async () => {
  const f = fixture(),
    result = f.session.open(1, f.current);
  await tick();
  assert.equal(f.session.canApprove(), false);
  await f.session.approve();
  await f.session.approve('ph://foreign');
  f.session.preview('ph://foreign');
  assert.equal(f.session.getSnapshot().screen, 'grid');
  assert.deepEqual(f.copied, []);
  f.session.preview('ph://a');
  assert.equal(f.session.canApprove('ph://a'), true);
  await f.session.approve('ph://a');
  assert.deepEqual(f.copied, [['ph://a']]);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(await result, [file('ph://a')]);
});

test('full selection rejects an unselected gallery item but still permits sending a selected current item', async () => {
  const f = fixture(),
    result = f.session.open(1, f.current);
  await tick();
  f.session.select(item('a'));
  f.session.preview('ph://b');
  assert.equal(f.session.canApprove('ph://b'), false);
  await f.session.approve('ph://b');
  assert.deepEqual(f.copied, []);
  assert.equal(f.session.getSnapshot().visible, true);
  assert.equal(f.session.canApprove('ph://a'), true);
  await f.session.approve('ph://a');
  assert.deepEqual(f.copied, [['ph://a']]);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(await result, [file('ph://a')]);
});

test('selected gallery accepts only an exact reorder permutation and preserves item identity across albums', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.select(item('a'));
  f.session.album('second');
  await tick();
  f.session.select(item('d'));
  const selected = f.session.getSnapshot().selected;
  f.session.showSelected();
  assert.equal(f.session.getSnapshot().screen, 'selected');
  assert.equal(f.session.getSnapshot().previewId, 'ph://b');
  for (const ids of [
    [],
    ['ph://b'],
    ['ph://b', 'ph://b', 'ph://d'],
    ['ph://b', 'ph://a', 'ph://foreign'],
    ['ph://b', 'ph://a', 'ph://d', 'ph://e'],
  ]) {
    f.session.reorder(ids);
    assert.equal(f.session.getSnapshot().selected, selected);
  }
  f.session.reorder(['ph://d', 'ph://b', 'ph://a']);
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://d', 'ph://b', 'ph://a'],
  );
  assert.equal(f.session.getSnapshot().selected[1], selected[0]);
  assert.equal(f.session.getSnapshot().selected[2], selected[1]);
  assert.equal(f.session.getSnapshot().previewId, 'ph://b');
  await f.session.approve();
  assert.deepEqual(f.copied, [['ph://d', 'ph://b', 'ph://a']]);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(await result, [file('ph://d'), file('ph://b'), file('ph://a')]);
});

test('selected gallery removal moves focus to a remaining item and returns to grid only after the last removal', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.select(item('a'));
  f.session.preview('ph://a');
  f.session.showSelected();
  assert.equal(f.session.getSnapshot().previewId, 'ph://a');
  f.session.select(item('a'));
  assert.equal(f.session.getSnapshot().screen, 'selected');
  assert.equal(f.session.getSnapshot().previewId, 'ph://b');
  f.session.select(item('b'));
  assert.equal(f.session.getSnapshot().screen, 'grid');
  assert.equal(f.session.getSnapshot().previewId, null);
  assert.deepEqual(f.copied, []);
  f.session.cancel();
  await result;
});

test('album details deduplicate requests and stale owner completions cannot overwrite or release a newer owner', async () => {
  const f = fixture(),
    firstDetails = deferred<{ count: number; previewId: string | null }>(),
    nextDetails = deferred<{ count: number; previewId: string | null }>();
  let calls = 0;
  f.port.albumDetails = () => (++calls === 1 ? firstDetails.promise : nextDetails.promise);
  const first = f.session.open(3, f.current);
  await tick();
  f.session.showAlbums();
  const oldWork = f.session.albumDetails('first');
  await f.session.albumDetails('first');
  await f.session.albumDetails('foreign');
  assert.equal(calls, 1);
  f.session.cancel();
  await first;
  const next = f.session.open(3, f.current);
  await tick();
  f.session.showAlbums();
  const newWork = f.session.albumDetails('first');
  firstDetails.resolve({ count: 999, previewId: 'ph://old-account' });
  await oldWork;
  assert.equal(f.session.getSnapshot().albums[0]!.count, undefined);
  await f.session.albumDetails('first');
  assert.equal(calls, 2, 'Old finally must not remove the replacement request deduplication owner');
  nextDetails.resolve({ count: 3, previewId: 'ph://current' });
  await newWork;
  assert.equal(f.session.getSnapshot().albums[0]!.count, 3);
  assert.equal(f.session.getSnapshot().albums[0]!.previewId, 'ph://current');
  await f.session.albumDetails('first');
  assert.equal(calls, 2, 'Known details do not query again');
  f.session.cancel();
  await next;
});

test('album details ignore retired epochs and invalid metadata while keeping picker interaction available', async () => {
  const f = fixture(),
    held = deferred<{ count: number; previewId: string | null }>();
  f.port.albumDetails = () => held.promise;
  const result = f.session.open(3, f.current);
  await tick();
  f.session.showAlbums();
  const oldWork = f.session.albumDetails('first');
  f.session.album('second');
  await tick();
  held.resolve({ count: 999, previewId: 'ph://old-page' });
  await oldWork;
  assert.equal(f.session.getSnapshot().albumId, 'second');
  assert.equal(f.session.getSnapshot().albums[0]!.count, undefined);
  for (const invalid of [
    { count: -1, previewId: 'ph://a' },
    { count: 0.5, previewId: null },
    { count: 1, previewId: 'file:///foreign' },
  ]) {
    f.session.showAlbums();
    f.port.albumDetails = async () => invalid;
    await f.session.albumDetails('first');
    assert.equal(f.session.getSnapshot().albums[0]!.count, undefined);
    assert.equal(f.session.getSnapshot().error, null);
    assert.equal(f.session.isInteractive(f.session.getSnapshot().token), true);
  }
  f.session.cancel();
  await result;
});

test('Back restores the gallery entry screen across page changes and resets origin for a new Photos request', async () => {
  const f = fixture(),
    first = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.select(item('a'));
  f.session.showSelected();
  f.session.preview('ph://a');
  f.session.preview('ph://b');
  f.session.back();
  assert.equal(f.session.getSnapshot().screen, 'selected');
  assert.equal(f.session.getSnapshot().previewId, 'ph://b');
  f.session.back();
  assert.equal(f.session.getSnapshot().screen, 'grid');
  f.session.preview('ph://a');
  f.session.back();
  assert.equal(f.session.getSnapshot().screen, 'grid');
  f.session.showSelected();
  f.session.preview('ph://a');
  f.session.cancel();
  await first;
  const next = f.session.open(3, f.current);
  await tick();
  f.session.preview('ph://b');
  f.session.back();
  assert.equal(f.session.getSnapshot().screen, 'grid');
  assert.equal(f.session.getSnapshot().previewId, null);
  assert.deepEqual(f.copied, []);
  f.session.cancel();
  await next;
});
