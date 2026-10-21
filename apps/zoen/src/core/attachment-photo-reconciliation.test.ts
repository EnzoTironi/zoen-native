import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentPhotoSession, type PhotoPickerPort } from '../media/attachment-photo-session.ts';
import type { PhotoLibraryItem, PhotoLibraryPermission } from '../media/attachment-photo-library';

const item = (name: string): PhotoLibraryItem => ({
  id: `ph://${name}`,
  filename: `${name}.jpg`,
  mediaType: 'image' as PhotoLibraryItem['mediaType'],
  width: 100,
  height: 200,
  duration: null,
  creationTime: null,
  modificationTime: null,
  isFavorite: false,
});
const allowed: PhotoLibraryPermission = {
  granted: true,
  status: 'granted' as PhotoLibraryPermission['status'],
  canAskAgain: true,
  accessPrivileges: 'all',
};
const denied: PhotoLibraryPermission = {
  granted: false,
  status: 'denied' as PhotoLibraryPermission['status'],
  canAskAgain: false,
  accessPrivileges: 'none',
};
const tick = async () => {
  for (let i = 0; i < 24; i++) await Promise.resolve();
};
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
function fixture() {
  let current = true;
  const refreshed: string[][] = [],
    offsets: number[] = [];
  const port: PhotoPickerPort = {
    permission: async () => allowed,
    requestPermission: async () => {
      throw new Error('Unexpected permission prompt');
    },
    load: async (_album, offset) => {
      offsets.push(offset);
      return [item('a'), item('b'), item('c')];
    },
    albums: async () => [{ id: 'other', title: 'Other' }],
    refresh: async (items) => {
      refreshed.push(items.map((value) => value.id));
      return [...items];
    },
    manage: async () => {},
    materialize: async () => [],
    capture: async () => [],
    discard: () => {},
  };
  const session = new AttachmentPhotoSession(port);
  return {
    session,
    port,
    refreshed,
    offsets,
    current: () => current,
    retire: () => {
      current = false;
      session.cancel();
    },
  };
}

test('unrelated library changes preserve cross-album ordered selection and refresh its real metadata', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.select(item('a'));
  f.session.preview('ph://b');
  f.port.load = async () => [item('new')];
  f.port.refresh = async (values) =>
    values.map((value) => ({ ...value, filename: `updated-${value.filename}`, width: 450 }));
  f.session.invalidate();
  assert.equal(f.session.canApprove('ph://b'), false);
  await tick();
  const state = f.session.getSnapshot();
  assert.deepEqual(
    state.selected.map((value) => value.id),
    ['ph://b', 'ph://a'],
  );
  assert.ok(
    state.selected.every((value) => value.width === 450 && value.filename?.startsWith('updated-')),
  );
  assert.equal(state.previewId, 'ph://b');
  assert.equal(state.screen, 'preview');
  assert.equal(state.items.find((value) => value.id === 'ph://b')?.width, 450);
  assert.equal(f.session.canApprove('ph://b'), true);
  f.session.cancel();
  await result;
});

test('missing selected/current media is removed without erasing the surviving selection or showing a ghost preview', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.select(item('a'));
  f.session.preview('ph://b');
  f.port.refresh = async (values) => values.filter((value) => value.id !== 'ph://b');
  f.session.invalidate();
  await tick();
  const state = f.session.getSnapshot();
  assert.deepEqual(
    state.selected.map((value) => value.id),
    ['ph://a'],
  );
  assert.equal(
    state.items.some((value) => value.id === 'ph://b'),
    false,
  );
  assert.equal(state.previewId, null);
  assert.equal(state.screen, 'grid');
  assert.equal(f.session.canApprove(), true);
  f.session.cancel();
  await result;
});

test('a transient metadata failure retains selection and requires successful reconciliation before Send', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current),
    failure = new Error('Read interrupted');
  await tick();
  f.session.select(item('b'));
  f.port.refresh = async () => {
    throw failure;
  };
  f.session.invalidate();
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  assert.equal(f.session.getSnapshot().error, failure);
  assert.equal(f.session.canApprove(), false);
  f.port.refresh = async (values) => [...values];
  await f.session.reload();
  assert.equal(f.session.getSnapshot().error, null);
  assert.equal(f.session.canApprove(), true);
  f.session.cancel();
  await result;
});

test('late reconciliation cannot reselect an item deselected during the read or undo the latest order', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current),
    read = deferred<PhotoLibraryItem[]>();
  await tick();
  f.session.select(item('a'));
  f.session.select(item('b'));
  f.session.select(item('c'));
  f.port.refresh = () => read.promise;
  f.session.invalidate();
  await tick();
  f.session.select(item('b'));
  f.session.reorder(['ph://c', 'ph://a']);
  read.resolve([item('a'), item('b'), item('c')]);
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://c', 'ph://a'],
  );
  f.session.cancel();
  await result;
});

test('an item selected after refresh started must be verified before Send becomes available', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current),
    read = deferred<PhotoLibraryItem[]>();
  await tick();
  f.session.select(item('a'));
  let calls = 0;
  f.port.refresh = async (values) => {
    calls++;
    return calls === 1 ? read.promise : values.filter((value) => value.id !== 'ph://c');
  };
  f.session.invalidate();
  await tick();
  f.session.select(item('c'));
  read.resolve([item('a')]);
  await tick();
  assert.equal(calls, 2);
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  assert.equal(f.session.canApprove(), true);
  f.session.cancel();
  await result;
});

test('permission revoked during reconciliation clears media and rejects the older metadata', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current),
    read = deferred<PhotoLibraryItem[]>();
  await tick();
  f.session.select(item('a'));
  f.session.preview('ph://a');
  f.port.refresh = () => read.promise;
  f.session.invalidate();
  await tick();
  f.port.permission = async () => denied;
  read.resolve([item('a')]);
  await tick();
  assert.deepEqual(f.session.getSnapshot().selected, []);
  assert.deepEqual(f.session.getSnapshot().items, []);
  assert.equal(f.session.getSnapshot().permission?.granted, false);
  assert.equal(f.session.getSnapshot().previewId, null);
  assert.equal(f.session.canApprove('ph://a'), false);
  f.session.cancel();
  await result;
});

test('an explicit reload also reconciles a changed permission grant before retaining selections', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('a'));
  f.session.select(item('b'));
  f.port.permission = async () => ({ ...allowed, accessPrivileges: 'limited' });
  f.port.refresh = async (values) => values.filter((value) => value.id === 'ph://b');
  await f.session.reload();
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  assert.equal(f.session.getSnapshot().permission?.accessPrivileges, 'limited');
  f.session.cancel();
  await result;
});

test('permission narrowing restarts bounded identity verification instead of applying metadata from the old grant', async () => {
  const f = fixture(),
    result = f.session.open(3, f.current),
    read = deferred<PhotoLibraryItem[]>();
  await tick();
  f.session.select(item('a'));
  f.session.select(item('b'));
  let calls = 0;
  f.port.refresh = async (values) => {
    calls++;
    return calls === 1 ? read.promise : values.filter((value) => value.id === 'ph://b');
  };
  f.session.invalidate();
  await tick();
  f.port.permission = async () => ({ ...allowed, accessPrivileges: 'limited' });
  read.resolve([item('a'), item('b')]);
  await tick();
  assert.equal(calls, 2);
  assert.equal(f.session.getSnapshot().permission?.accessPrivileges, 'limited');
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  f.session.cancel();
  await result;
});

test('an old refresh cannot alter a replacement request or resurrect media after account retirement', async () => {
  const f = fixture(),
    first = f.session.open(3, f.current),
    read = deferred<PhotoLibraryItem[]>();
  await tick();
  f.session.select(item('a'));
  f.port.refresh = () => read.promise;
  f.session.invalidate();
  await tick();
  f.session.cancel();
  await first;
  const second = f.session.open(3, f.current);
  await tick();
  f.session.select(item('c'));
  read.resolve([item('a')]);
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().selected.map((value) => value.id),
    ['ph://c'],
  );
  f.retire();
  await second;
  await tick();
  assert.equal(f.session.getSnapshot().visible, false);
});

test('retaining an authorized preview beyond the first page does not skip a row on pagination', async () => {
  const f = fixture();
  const firstPage = Array.from({ length: 60 }, (_, index) => item(`page-${index}`));
  f.port.load = async (_album, offset) => {
    f.offsets.push(offset);
    return offset === 0 ? firstPage : [item('later')];
  };
  const result = f.session.open(3, f.current);
  await tick();
  await f.session.more();
  f.session.preview('ph://later');
  f.session.invalidate();
  await tick();
  assert.equal(f.session.getSnapshot().previewId, 'ph://later');
  assert.equal(f.session.getSnapshot().items.length, 61);
  await f.session.more();
  assert.equal(f.offsets.at(-1), 60);
  assert.equal(f.session.getSnapshot().items.length, 61);
  f.session.cancel();
  await result;
});
