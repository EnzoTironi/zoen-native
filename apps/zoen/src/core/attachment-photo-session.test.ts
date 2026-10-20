import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentPhotoSession, type PhotoPickerPort } from '../media/attachment-photo-session.ts';
import type { PhotoLibraryPermission, PhotoLibraryItem } from '../media/attachment-photo-library';
import type { AttachmentAsset } from '../media/attachment-draft';
const item = (id: string) =>
  ({
    id,
    filename: `${id}.jpg`,
    mediaType: 'image',
    width: 100,
    height: 200,
    duration: null,
    creationTime: 10,
    modificationTime: null,
    isFavorite: false,
  }) as PhotoLibraryItem;
const file = (id: string): AttachmentAsset => ({
  uri: `file:///cache/owned-${id}`,
  name: `${id}.jpg`,
  mime: 'image/jpeg',
  size: 10,
});
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
const tick = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
};
function fixture() {
  const discarded: string[] = [],
    copied: string[][] = [],
    captures: number[] = [];
  let current = true;
  const port: PhotoPickerPort = {
    permission: async () => ({
      status: 'granted' as PhotoLibraryPermission['status'],
      granted: true,
      canAskAgain: true,
      accessPrivileges: 'all',
    }),
    requestPermission: async () => {
      throw new Error('unexpected permission prompt');
    },
    albums: async () => [{ id: 'album', title: 'Album' }],
    load: async () => ['a', 'b', 'c'].map(item),
    manage: async () => {},
    materialize: async (items) => {
      copied.push(items.map((value) => value.id));
      return items.map((value) => file(value.id));
    },
    capture: async (remaining) => {
      captures.push(remaining);
      return [file('camera')];
    },
    discard: (uri) => discarded.push(uri),
  };
  const session = new AttachmentPhotoSession(port);
  return {
    session,
    port,
    discarded,
    copied,
    captures,
    current: () => current,
    retire: () => {
      current = false;
    },
  };
}
test('library opens only on explicit request and preserves bounded tap order across albums before dismissal handoff', async () => {
  const f = fixture();
  assert.equal(f.session.getSnapshot().visible, false);
  const result = f.session.open(2, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.select(item('a'));
  f.session.select(item('c'));
  assert.deepEqual(
    f.session.getSnapshot().selected.map((i) => i.id),
    ['b', 'a'],
  );
  f.session.select(item('b'));
  f.session.select(item('c'));
  assert.deepEqual(
    f.session.getSnapshot().selected.map((i) => i.id),
    ['a', 'c'],
  );
  f.session.album('album');
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().selected.map((i) => i.id),
    ['a', 'c'],
  );
  let resolved = false;
  void result.then(() => {
    resolved = true;
  });
  await f.session.approve();
  await tick();
  assert.equal(resolved, false);
  assert.equal(f.session.getSnapshot().visible, false);
  assert.deepEqual(f.copied, [['a', 'c']]);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(await result, [file('a'), file('c')]);
  assert.deepEqual(f.discarded, []);
});
test('permission completion after cancellation cannot enumerate Photos or mutate a replacement request', async () => {
  const f = fixture(),
    permission = deferred<Awaited<ReturnType<PhotoPickerPort['permission']>>>();
  f.port.permission = () => permission.promise;
  let queries = 0;
  f.port.load = async () => {
    queries++;
    return [];
  };
  const first = f.session.open(3, f.current),
    old = f.session.getSnapshot().token;
  f.session.cancel();
  assert.deepEqual(await first, []);
  f.port.permission = async () => ({
    status: 'denied' as PhotoLibraryPermission['status'],
    granted: false,
    canAskAgain: false,
    accessPrivileges: 'none',
  });
  const second = f.session.open(3, f.current);
  await tick();
  permission.resolve({
    status: 'granted' as PhotoLibraryPermission['status'],
    granted: true,
    canAskAgain: true,
    accessPrivileges: 'all',
  });
  await tick();
  await f.session.dismissed(old);
  assert.equal(queries, 0);
  assert.equal(f.session.getSnapshot().visible, true);
  assert.equal(f.session.getSnapshot().permission?.granted, false);
  f.session.cancel();
  await second;
});
test('undetermined permission requests once from Photos admission, denied has no enumeration and limited selector refreshes', async () => {
  const f = fixture();
  let prompted = 0,
    managed = 0,
    loads = 0;
  f.port.permission = async () => ({
    status: 'undetermined' as PhotoLibraryPermission['status'],
    granted: false,
    canAskAgain: true,
  });
  f.port.requestPermission = async () => {
    prompted++;
    return {
      status: 'granted' as PhotoLibraryPermission['status'],
      granted: true,
      canAskAgain: true,
      accessPrivileges: 'limited',
    };
  };
  f.port.load = async () => {
    loads++;
    return [item('a')];
  };
  f.port.manage = async () => {
    managed++;
  };
  const result = f.session.open(2, f.current);
  await tick();
  assert.equal(prompted, 1);
  assert.equal(loads, 1);
  f.session.select(item('a'));
  await f.session.manage();
  await tick();
  assert.equal(managed, 1);
  assert.deepEqual(f.session.getSnapshot().selected, []);
  // Checking permissions after the native limited selector never repeats the initial prompt.
  assert.equal(prompted, 1);
  assert.equal(loads, 1);
  f.session.cancel();
  await result;
});
test('new album query fences old asynchronous page results without losing the selected order', async () => {
  const f = fixture(),
    held = deferred<PhotoLibraryItem[]>();
  f.port.load = (id) => (id === null ? held.promise : Promise.resolve([item('album-only')]));
  const result = f.session.open(2, f.current);
  await tick();
  f.session.album('album');
  await tick();
  held.resolve([item('old')]);
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().items.map((i) => i.id),
    ['album-only'],
  );
  f.session.cancel();
  await result;
});
test('background or account retirement during native copy holds the drain and discards every late owned copy', async () => {
  const f = fixture(),
    held = deferred<AttachmentAsset[]>();
  f.port.materialize = () => held.promise;
  const result = f.session.open(2, f.current);
  await tick();
  f.session.select(item('a'));
  const work = f.session.approve();
  f.session.cancel();
  let finished = false;
  void result.then(() => {
    finished = true;
  });
  await tick();
  assert.equal(finished, false);
  held.resolve([file('late')]);
  await work;
  assert.deepEqual(await result, []);
  assert.deepEqual(f.discarded, [file('late').uri]);
});
test('camera waits for real sheet dismissal, returns selected files first, and camera cancel restores selection', async () => {
  const f = fixture();
  const result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('b'));
  f.session.camera();
  assert.deepEqual(f.captures, []);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(f.captures, [2]);
  assert.deepEqual(await result, [file('b'), file('camera')]);
  f.port.capture = async () => [];
  const canceled = f.session.open(3, f.current);
  await tick();
  f.session.select(item('a'));
  f.session.camera();
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.equal(f.session.getSnapshot().visible, true);
  assert.deepEqual(
    f.session.getSnapshot().selected.map((i) => i.id),
    ['a'],
  );
  f.session.cancel();
  await canceled;
});
test('a failed copy keeps selection for retry and an explicit Close never stages or sends anything', async () => {
  const f = fixture();
  f.port.materialize = async () => {
    throw new Error('cloud unavailable');
  };
  const result = f.session.open(2, f.current);
  await tick();
  f.session.select(item('c'));
  await f.session.approve();
  assert.equal(f.session.getSnapshot().visible, true);
  assert.ok(f.session.getSnapshot().error);
  assert.deepEqual(
    f.session.getSnapshot().selected.map((i) => i.id),
    ['c'],
  );
  f.session.cancel();
  assert.deepEqual(await result, []);
  assert.deepEqual(f.discarded, []);
});

test('retained Review and Camera callbacks cannot create a second copy or overwrite a hidden handoff', async () => {
  const f = fixture();
  const result = f.session.open(3, f.current);
  await tick();
  f.session.select(item('a'));
  await f.session.approve();
  await f.session.approve();
  f.session.camera();
  f.session.select(item('b'));
  assert.equal(f.session.isInteractive(f.session.getSnapshot().token), false);
  assert.deepEqual(f.copied, [['a']]);
  assert.deepEqual(f.captures, []);
  await f.session.dismissed(f.session.getSnapshot().token);
  assert.deepEqual(await result, [file('a')]);
});

test('presentation-state change revokes user dismissal immediately but never starts an approved camera handoff', async () => {
  const f = fixture();
  const result = f.session.open(3, f.current);
  await tick();
  const token = f.session.getSnapshot().token;
  f.session.select(item('a'));
  f.session.camera();
  f.session.dismissing(token);
  assert.deepEqual(f.captures, []);
  await f.session.dismissed(token);
  assert.deepEqual(await result, [file('a'), file('camera')]);
  const canceled = f.session.open(3, f.current);
  await tick();
  f.session.select(item('a'));
  f.session.dismissing(f.session.getSnapshot().token);
  f.session.select(item('b'));
  await f.session.approve();
  assert.deepEqual(await canceled, []);
  assert.deepEqual(f.copied, [['a']]);
});
