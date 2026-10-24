import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import { AttachmentPhotoSession } from '../media/attachment-photo-session.ts';
import type { PhotoLibraryItem, PhotoLibraryPermission } from '../media/attachment-photo-library';
import { fixture as composerFixture } from './attachment-composer-ui.fixtures.ts';
import {
  deferred,
  empty,
  fixture,
  photo,
  scope,
  tick,
} from './durable-attachment-composer.fixtures.ts';

const image = (id: string): PhotoLibraryItem => ({
  id: `ph://${id}`,
  filename: `${id}.jpg`,
  mediaType: 'image' as PhotoLibraryItem['mediaType'],
  width: 100,
  height: 200,
  duration: null,
  creationTime: null,
  modificationTime: null,
  isFavorite: false,
  mediaSubtypes: [],
});

for (const existing of [false, true]) {
  test(`journaled Photos changes preserve the actual ${existing ? 'caption' : 'message'} field owner until native removal`, async () => {
    const f = fixture();
    f.render();
    await tick();
    if (existing) {
      f.select([photo('existing.jpg')]);
      await f.render().pick('photo');
      f.render().field!.onChange('Existing caption');
      await f.channel.flush();
    }
    let token = 0;
    const keyboard = new AttachmentKeyboardSession('chat', true, () => `anchor-${++token}`);
    const composer = composerFixture('ios', 1, false, keyboard);
    const photos = new AttachmentPhotoSession({
      permission: async () => ({
        status: 'granted' as PhotoLibraryPermission['status'],
        granted: true,
        canAskAgain: true,
        accessPrivileges: 'all',
      }),
      requestPermission: async () => {
        throw Error('Already granted');
      },
      load: async () => [image('a'), image('b')],
      refresh: async (items) => [...items],
      albums: async () => [],
      manage: async () => {},
      materialize: async (items) => items.map((item) => photo(item.filename ?? 'image.jpg')),
      capture: async () => [],
      discard: () => {},
    });
    f.nativePhotoPicker(
      (remaining, current, admission) => photos.open(remaining, current, admission?.journal),
      () => photos.retire(),
    );
    const render = () => {
      const ui = f.render();
      composer.render(ui.field, { spaceId: 'chat' });
      return ui;
    };
    const initialField = render().field;
    const anchored = keyboard.getSnapshot().field;
    assert.ok(anchored);
    let picking: Promise<void> | undefined;
    keyboard.register(async () => {
      picking = f.render().pick('photo');
      await picking;
    }, true);
    keyboard.openAttachments();
    const owner = keyboard.getSnapshot().presentation!.id;
    assert.equal(keyboard.admitPicker(owner), true);
    await tick();
    render();
    assert.equal(keyboard.ownsPicker(owner), true);
    photos.select(image('a'));
    await tick();
    await f.channel.flush();
    await f.render().pick('photo');
    const selected = render();
    assert.equal(
      JSON.parse(f.durable().content_json).items.length,
      existing ? 2 : 1,
      'selection is durable before closing',
    );
    assert.equal(
      selected.field?.total,
      initialField?.total,
      'the underlying composer keeps its admitted caption count while Photos owns it',
    );
    assert.equal(
      keyboard.getSnapshot().field,
      anchored,
      'journal publication cannot replace the native anchor',
    );
    assert.equal(
      keyboard.ownsPicker(owner),
      true,
      'first selection cannot retire the active Photos task',
    );
    photos.select(image('b'));
    photos.setCaptions({ forced: null, items: { 'ph://a': 'A caption', 'ph://b': 'B caption' } });
    photos.reorder(['ph://b', 'ph://a']);
    await tick();
    await f.channel.flush();
    const held = render();
    held.field?.onChange('Stale composer caption');
    held.field?.onSend();
    await tick();
    assert.equal(
      keyboard.getSnapshot().field,
      anchored,
      'order, count and captions retain the same anchor',
    );
    assert.equal(keyboard.ownsPicker(owner), true);
    assert.equal(f.sendCalls(), 0);
    assert.deepEqual(
      JSON.parse(f.durable().content_json)
        .items.slice(existing ? 1 : 0)
        .map((item: { caption: string }) => item.caption),
      ['B caption', 'A caption'],
    );
    keyboard.closed(owner);
    photos.retire();
    await picking;
    await tick();
    const settled = render();
    assert.equal(keyboard.ownsPicker(owner), false);
    assert.equal(
      settled.field?.total,
      existing ? 3 : 2,
      'the drained picker publishes the durable caption selection',
    );
    assert.equal(f.sendCalls(), 0);
    composer.unmount();
    f.unmount();
  });
}

test('late removal from the previous conversation cannot release the new Photos anchor', async () => {
  const f = fixture();
  const other = fixture(empty(), { ...scope, space_id: 'other' });
  const removed = deferred<ReturnType<typeof photo>[]>();
  let admissions = 0;
  const photos = new AttachmentPhotoSession({
    permission: async () => ({
      status: 'granted' as PhotoLibraryPermission['status'],
      granted: true,
      canAskAgain: true,
      accessPrivileges: 'all',
    }),
    requestPermission: async () => {
      throw Error('Already granted');
    },
    load: async () => [image('new')],
    refresh: async (items) => [...items],
    albums: async () => [],
    manage: async () => {},
    materialize: async () => [photo('new.jpg')],
    capture: async () => [],
    discard: () => {},
  });
  f.nativePhotoPicker((remaining, current, admission) =>
    ++admissions === 1 ? removed.promise : photos.open(remaining, current, admission?.journal),
  );
  f.render();
  await tick();
  const oldPick = f.render().pick('photo');
  await tick();
  f.render({ spaceId: 'other' }, other.channel);
  await tick();
  let token = 0;
  const keyboard = new AttachmentKeyboardSession('other', true, () => `replacement-${++token}`);
  const composer = composerFixture('ios', 1, false, keyboard);
  const render = () => {
    const ui = f.render({ spaceId: 'other' }, other.channel);
    composer.render(ui.field, { spaceId: 'other' });
    return ui;
  };
  render();
  const anchored = keyboard.getSnapshot().field;
  let newPick: Promise<void> | undefined;
  keyboard.register(async () => {
    newPick = render().pick('photo');
    await newPick;
  }, true);
  keyboard.openAttachments();
  const owner = keyboard.getSnapshot().presentation!.id;
  keyboard.admitPicker(owner);
  await tick();
  render();
  photos.select(image('new'));
  await tick();
  await other.channel.flush();
  render();
  assert.equal(keyboard.getSnapshot().field, anchored);
  removed.resolve([]);
  await oldPick;
  render();
  assert.equal(
    keyboard.getSnapshot().field,
    anchored,
    "the old task cannot clear another channel's anchor",
  );
  assert.equal(keyboard.ownsPicker(owner), true);
  assert.equal(JSON.parse(other.durable().content_json).items.length, 1);
  assert.equal(f.durable().content_json, 'null');
  assert.equal(other.sendCalls(), 0);
  assert.equal(f.sendCalls(), 0);
  keyboard.closed(owner);
  photos.retire();
  await newPick;
  composer.unmount();
  f.unmount();
  other.unmount();
});
