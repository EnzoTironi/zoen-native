import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentPhotoSession, type PhotoPickerPort } from '../media/attachment-photo-session.ts';
import type { PhotoLibraryPermission, PhotoLibraryItem } from '../media/attachment-photo-library';
const tick = async () => {
  for (let index = 0; index < 15; index++) await Promise.resolve();
};
async function fixture() {
  let current = true;
  let copies = 0;
  const item = {
    id: 'synthetic-photo',
    filename: 'photo.jpg',
    mediaType: 'image',
    width: 20,
    height: 20,
    duration: null,
    creationTime: 1,
    modificationTime: null,
    isFavorite: false,
  } as PhotoLibraryItem;
  const permission = {
    status: 'granted',
    granted: true,
    canAskAgain: true,
    accessPrivileges: 'all',
  } as PhotoLibraryPermission;
  const port: PhotoPickerPort = {
    permission: async () => permission,
    requestPermission: async () => permission,
    load: async () => [item],
    refresh: async (items) => [...items],
    albums: async () => [],
    manage: async () => {},
    materialize: async () => {
      copies++;
      return [];
    },
    capture: async () => {
      throw Error('not used');
    },
    discard: () => {
      throw Error('not used');
    },
  };
  const session = new AttachmentPhotoSession(port);
  const result = session.open(10, () => current);
  await tick();
  return {
    session,
    result,
    item,
    copies: () => copies,
    retire: () => {
      current = false;
    },
  };
}

test('category handoff waits for the exact native close, runs once, and never materializes or sends Photos', async () => {
  const f = await fixture();
  const token = f.session.getSnapshot().token;
  const events: string[] = [];
  f.session.openRoute((current) => {
    assert.equal(current(), true);
    events.push('route');
  });
  assert.equal(f.session.getSnapshot().visible, false);
  assert.deepEqual(events, []);
  await f.session.dismissed(token + 1);
  assert.deepEqual(events, []);
  await f.session.dismissed(token);
  assert.deepEqual(events, ['route']);
  assert.deepEqual(await f.result, []);
  await f.session.dismissed(token);
  assert.deepEqual(events, ['route']);
  assert.equal(f.copies(), 0);
});

test('a selected Photo cannot be silently discarded to open another category', async () => {
  const f = await fixture();
  f.session.select(f.item);
  let opens = 0;
  f.session.openRoute(() => {
    opens++;
  });
  assert.equal(f.session.getSnapshot().visible, true);
  assert.equal(f.session.getSnapshot().selected.length, 1);
  assert.equal(opens, 0);
  f.session.cancel();
  await f.result;
  assert.equal(f.copies(), 0);
});

test('background/account retirement and cancel revoke a queued route before native close', async () => {
  for (const cancel of [true, false]) {
    const f = await fixture();
    const token = f.session.getSnapshot().token;
    let opens = 0;
    f.session.openRoute(() => {
      opens++;
    });
    if (cancel) f.session.cancel();
    else f.retire();
    await f.session.dismissed(token);
    assert.equal(opens, 0);
    assert.deepEqual(await f.result, []);
  }
});

test('failed route presentation restores the same empty picker for explicit recovery, without replaying the action', async () => {
  const f = await fixture();
  const token = f.session.getSnapshot().token;
  let opens = 0;
  f.session.openRoute(() => {
    opens++;
    throw new Error('synthetic presentation refusal');
  });
  await f.session.dismissed(token);
  assert.equal(f.session.getSnapshot().visible, true);
  assert.equal(opens, 1);
  assert.match(String(f.session.getSnapshot().error), /synthetic presentation refusal/);
  f.session.cancel();
  await f.result;
  assert.equal(opens, 1);
  assert.equal(f.copies(), 0);
});
