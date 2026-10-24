import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentPhotoSession, type PhotoPickerPort } from '../media/attachment-photo-session.ts';
import type { PhotoLibraryItem, PhotoLibraryPermission } from '../media/attachment-photo-library';
import { fixture, findStrip, tick } from './durable-attachment-composer.fixtures.ts';

export const image: PhotoLibraryItem = {
  id: 'ph://retained',
  filename: 'waterfall.jpg',
  mediaType: 'image' as PhotoLibraryItem['mediaType'],
  width: 100,
  height: 200,
  duration: null,
  creationTime: null,
  modificationTime: null,
  isFavorite: false,
  mediaSubtypes: [],
};
export const photoPort = (): PhotoPickerPort => ({
  permission: async () => ({
    status: 'granted' as PhotoLibraryPermission['status'],
    granted: true,
    canAskAgain: true,
    accessPrivileges: 'all',
  }),
  requestPermission: async () => {
    throw Error('No permission prompt');
  },
  load: async () => [image],
  refresh: async (items) => [...items],
  albums: async () => [],
  manage: async () => {},
  materialize: async (items) =>
    items.map(() => ({
      uri: 'file:///gallery-owned.jpg',
      name: 'waterfall.jpg',
      mime: 'image/jpeg',
      size: 12,
    })),
  capture: async () => [],
  discard: () => {},
});

test('Photos selection is journaled before Edit or Send resolves and cold-reopens without sending', async () => {
  const f = fixture();
  const session = new AttachmentPhotoSession(photoPort());
  f.nativePhotoPicker((remaining, current, admission) =>
    session.open(remaining, current, admission?.journal),
  );
  f.render();
  await tick();
  const selecting = f.render().pick('photo');
  await tick();
  session.select(image);
  await tick();
  await f.channel.flush();
  const saved = f.durable();
  const cold = fixture(saved);
  cold.render();
  await tick();
  assert.equal(
    cold.render().active,
    true,
    'a selected photo must already be in the existing durable channel',
  );
  assert.equal(JSON.parse(saved.content_json).items.length, 1);
  assert.equal(f.sent.length, 0);
  assert.equal(cold.sent.length, 0);
  session.cancel();
  await selecting;
});

function connect(f: ReturnType<typeof fixture>, port = photoPort()) {
  const session = new AttachmentPhotoSession(port);
  f.nativePhotoPicker(
    (remaining, current, admission) => session.open(remaining, current, admission?.journal),
    () => session.retire(),
  );
  return session;
}
async function begin(f: ReturnType<typeof fixture>) {
  f.render();
  await tick();
  const done = f.render().pick('photo');
  await tick();
  return { done };
}
const content = (f: ReturnType<typeof fixture>) => JSON.parse(f.durable().content_json);

test('each gallery caption and order cold-restore to selected Photos without re-exporting original assets', async () => {
  const second = { ...image, id: 'ph://second', filename: 'second.jpg' };
  const f = fixture();
  let exports = 0;
  const port = photoPort();
  port.load = async () => [image, second];
  const original = port.materialize;
  port.materialize = async (...args) => {
    exports += args[0].length;
    return original(...args);
  };
  const session = connect(f, port);
  const { done } = await begin(f);
  session.select(image);
  session.select(second);
  session.setCaptions({
    forced: null,
    items: { [image.id]: 'Cachoeira 🌊', [second.id]: 'Segunda' },
  });
  session.reorder([second.id, image.id]);
  await tick();
  await f.channel.flush();
  await tick();
  assert.equal(
    session.getSnapshot().error,
    null,
    'superseded previews cannot report conflict after the latest order is saved',
  );
  assert.deepEqual(
    content(f).items.map((item: { caption: string }) => item.caption),
    ['Segunda', 'Cachoeira 🌊'],
  );
  assert.equal(exports, 2);
  const cold = fixture(f.durable());
  const restored = connect(cold, port);
  const reopened = await begin(cold);
  assert.deepEqual(
    restored.getSnapshot().selected.map((item) => item.id),
    [second.id, image.id],
  );
  assert.equal(restored.getSnapshot().captions.items[image.id], 'Cachoeira 🌊');
  assert.equal(exports, 2, 'cold hydration reads retained identities; no Photos export');
  assert.equal(cold.sent.length, 0);
  session.retire();
  restored.retire();
  await Promise.all([done, reopened.done]);
});

test('explicit Cancel removes only gallery additions and restores the pre-existing caption/reply/options', async () => {
  const baseline = {
    kind: 'attachments',
    reply_to: 'original-reply',
    options: { presentation: 'file', grouping: 'separate' },
    items: [{ asset_id: 'base', mutation_id: 'base-post', caption: 'Preserve me' }],
  };
  const f = fixture({
    revision: 'base-rev',
    content_json: JSON.stringify(baseline),
    attempted: false,
    assets: [
      {
        id: 'base',
        name: 'original.pdf',
        mime: 'application/pdf',
        size: 12,
        sha256: 'b'.repeat(64),
      },
    ],
  });
  const session = connect(f);
  const { done } = await begin(f);
  session.select(image);
  session.setCaptions({ forced: 'New caption', items: {} });
  await tick();
  await f.channel.flush();
  assert.equal(content(f).items.length, 2);
  session.cancel();
  await done;
  assert.deepEqual(content(f), baseline);
  assert.equal(f.sent.length, 0);
});

test('background retirement preserves the journaled forced caption while explicit Send reuses original asset IDs once', async () => {
  const f = fixture();
  const session = connect(f);
  const first = await begin(f);
  session.select(image);
  session.setCaptions({ forced: 'Mensagem pendente', items: {} });
  await tick();
  await f.channel.flush();
  const original = content(f).items[0].asset_id;
  f.background();
  session.retire();
  await first.done;
  assert.equal(content(f).items[0].caption, 'Mensagem pendente');
  const cold = fixture(f.durable());
  const restored = connect(cold);
  const next = await begin(cold);
  assert.equal(restored.getSnapshot().captions.forced, 'Mensagem pendente');
  await restored.approve(undefined, () =>
    cold.nativePhotoSubmission({
      caption: 'Mensagem pendente',
      captions: ['Mensagem pendente'],
      send: true,
    }),
  );
  await restored.dismissed(restored.getSnapshot().token);
  await next.done;
  assert.equal(cold.sent.length, 1);
  const sent = JSON.parse(cold.sent[0]!);
  assert.equal(sent.gallery, undefined);
  assert.equal(sent.items.length, 1);
  assert.equal(sent.items[0].asset_id, original);
  assert.equal(cold.sendCalls(), 1);
});

test('late Photos materialization after account retirement creates no draft and is cleaned', async () => {
  const f = fixture();
  const pending = Promise.withResolvers<Awaited<ReturnType<PhotoPickerPort['materialize']>>>();
  const discarded: string[] = [];
  const port = photoPort();
  port.materialize = () => pending.promise;
  port.discard = (uri) => discarded.push(uri);
  const session = connect(f, port);
  const { done } = await begin(f);
  session.select(image);
  await tick();
  f.retire();
  pending.resolve([{ uri: 'file:///late-copy', name: 'late.jpg', mime: 'image/jpeg', size: 12 }]);
  await tick();
  await done;
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.writes.length, 0);
  assert.equal(f.sent.length, 0);
  assert.deepEqual(discarded, ['file:///late-copy']);
});

test('caption bursts coalesce behind one Photos copy and explicit retry preserves the same bytes after save refusal', async () => {
  const f = fixture();
  const port = photoPort();
  const wait = Promise.withResolvers<Awaited<ReturnType<PhotoPickerPort['materialize']>>>();
  let copies = 0;
  port.materialize = async () => {
    copies++;
    return wait.promise;
  };
  const session = connect(f, port);
  const { done } = await begin(f);
  session.select(image);
  await tick();
  for (let index = 0; index < 100; index++)
    session.setCaptions({ forced: `Caption ${index}`, items: {} });
  f.saveFailure(true);
  wait.resolve([{ uri: 'file:///one-copy', name: 'waterfall.jpg', mime: 'image/jpeg', size: 12 }]);
  await tick();
  assert.equal(f.durable().content_json, 'null');
  assert.ok(session.getSnapshot().error);
  f.saveFailure(false);
  await session.retry();
  assert.equal(content(f).items[0].caption, 'Caption 99');
  assert.equal(copies, 1);
  assert.equal(f.writes.length, 1);
  assert.equal(f.sent.length, 0);
  session.retire();
  await done;
});

test('Cancel waits a pending original copy and never admits its late completion', async () => {
  const f = fixture();
  const port = photoPort();
  const copy = Promise.withResolvers<Awaited<ReturnType<PhotoPickerPort['materialize']>>>();
  const discarded: string[] = [];
  port.materialize = () => copy.promise;
  port.discard = (uri) => discarded.push(uri);
  const session = connect(f, port);
  const { done } = await begin(f);
  session.select(image);
  await tick();
  session.cancel();
  let completed = false;
  void done.then(() => {
    completed = true;
  });
  await tick();
  assert.equal(completed, false);
  copy.resolve([
    { uri: 'file:///cancelled-original', name: 'waterfall.jpg', mime: 'image/jpeg', size: 12 },
  ]);
  await done;
  await f.channel.flush();
  assert.equal(f.writes.length, 0);
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.sent.length, 0);
  assert.deepEqual(discarded, ['file:///cancelled-original']);
});

test('restored gallery Edit moves the same canonical selection into the existing edited-draft flow', async () => {
  const first = fixture();
  const photos = connect(first);
  const before = await begin(first);
  photos.select(image);
  photos.setCaptions({ forced: 'Keep caption through Crop', items: {} });
  await tick();
  await first.channel.flush();
  photos.retire();
  await before.done;
  const cold = fixture(first.durable());
  cold.enableNativeEditing();
  const session = connect(cold);
  const resumed = await begin(cold);
  await session.approve(undefined, () =>
    cold.nativePhotoSubmission({
      caption: 'Keep caption through Crop',
      captions: ['Keep caption through Crop'],
      send: false,
      requestEdit: true,
    }),
  );
  await session.dismissed(session.getSnapshot().token);
  await cold.previewStarted;
  assert.equal(content(cold).gallery, undefined);
  cold.saveEditedCopy();
  await resumed.done;
  assert.equal(content(cold).items.length, 1);
  assert.match(content(cold).items[0].asset_id, /^edited-/);
  assert.equal(content(cold).items[0].caption, 'Keep caption through Crop');
  assert.equal(cold.sent.length, 0);
  const reopened = fixture(cold.durable());
  reopened.render();
  await tick();
  assert.equal(reopened.render().active, true);
});

test('explicit file/grouped submission after cold recovery preserves options, order and one send', async () => {
  for (const options of [
    { presentation: 'file', grouping: 'separate' },
    { presentation: 'media', grouping: 'grouped' },
  ] as const) {
    const second = { ...image, id: 'ph://second' };
    const port = photoPort();
    port.load = async () => [image, second];
    const f = fixture();
    const session = connect(f, port);
    const pending = await begin(f);
    session.select(image);
    session.select(second);
    await tick();
    await f.channel.flush();
    const ids = content(f).items.map((item: { asset_id: string }) => item.asset_id);
    session.retire();
    await pending.done;
    const cold = fixture(f.durable());
    const restored = connect(cold, port);
    const next = await begin(cold);
    await restored.approve(undefined, () =>
      cold.nativePhotoSubmission({
        caption: 'Final',
        captions: ['Final', 'Second'],
        send: true,
        options,
      }),
    );
    await restored.dismissed(restored.getSnapshot().token);
    await next.done;
    assert.equal(cold.sent.length, 1);
    const sent = JSON.parse(cold.sent[0]!);
    assert.deepEqual(sent.options, options);
    assert.equal(sent.gallery, undefined);
    assert.deepEqual(
      sent.items.map((item: { asset_id: string }) => item.asset_id),
      ids,
    );
    assert.deepEqual(
      sent.items.map((item: { caption: string }) => item.caption),
      ['Final', 'Second'],
    );
    assert.equal(cold.sendCalls(), 1);
  }
});

test('ordinary composer caption/removal can finish a recovered gallery without corrupting its remaining draft', async () => {
  const second = { ...image, id: 'ph://second' };
  const port = photoPort();
  port.load = async () => [image, second];
  const f = fixture();
  const session = connect(f, port);
  const pending = await begin(f);
  session.select(image);
  session.select(second);
  session.setCaptions({ forced: 'Common', items: {} });
  await tick();
  await f.channel.flush();
  session.retire();
  await pending.done;
  const cold = fixture(f.durable());
  cold.render();
  await tick();
  cold.render();
  await tick();
  const ids = content(cold).items.map((item: { asset_id: string }) => item.asset_id);
  const strip = findStrip(cold.render().review);
  (strip.props.onSelect as (id: string) => void)(ids[1]);
  cold.render().field!.onChange('Per-item after reopen');
  await cold.channel.flush();
  assert.equal(content(cold).gallery.forced_caption, false);
  (findStrip(cold.render().review).props.onRemove as (id: string) => void)(ids[0]);
  await tick();
  await cold.channel.flush();
  assert.equal(content(cold).gallery, undefined);
  assert.equal(content(cold).items.length, 1);
  assert.equal(content(cold).items[0].caption, 'Per-item after reopen');
  assert.equal(cold.sent.length, 0);
});

test('captured gallery previews and explicit Send use the same retained asset after the Photos item changes', async () => {
  const f = fixture();
  const port = photoPort();
  let exports = 0;
  const exportPhoto = port.materialize;
  port.materialize = async (...args) => {
    exports++;
    return exportPhoto(...args);
  };
  const session = connect(f, port);
  const { done } = await begin(f);
  session.select(image);
  await tick();
  await f.channel.flush();
  await tick();
  const uri = session.getSnapshot().previewUris[image.id];
  assert.ok(uri, 'selected pixels must come from the authorized durable journal copy');
  assert.notEqual(uri, 'file:///gallery-owned.jpg', 'never reuse the disposable Photos export');
  for (let i = 0; i < 100; i++) session.setCaptions({ forced: String(i), items: {} });
  await tick();
  await f.channel.flush();
  await tick();
  assert.equal(
    f.materializations.length,
    1,
    'caption metadata never re-materializes captured pixels',
  );
  session.retire();
  await done;
  const cold = fixture(f.durable());
  port.load = async () => [{ ...image, filename: 'externally-edited.jpg', width: 999 }];
  port.refresh = async () => [{ ...image, filename: 'externally-edited.jpg', width: 999 }];
  const restored = connect(cold, port);
  let handed: { uri: string }[] = [];
  cold.nativePhotoPicker(
    (remaining, current, admission) =>
      restored.open(remaining, current, admission?.journal).then((files) => {
        handed = files;
        return files;
      }),
    () => restored.retire(),
  );
  const reopened = await begin(cold);
  await tick();
  assert.equal(restored.getSnapshot().selected[0]?.filename, 'waterfall.jpg');
  const retainedUri = restored.getSnapshot().previewUris[image.id];
  assert.ok(retainedUri);
  await restored.approve();
  await restored.dismissed(restored.getSnapshot().token);
  await reopened.done;
  assert.equal(exports, 1, 'no re-export of an externally changed PHAsset');
  assert.equal(handed[0]?.uri, retainedUri, 'handoff consumes the exact displayed cache lease');
  assert.equal(cold.sent.length, 0);
});

test('cold retained selection survives Photos deletion or permission removal without reading a replacement', async () => {
  const f = fixture();
  const session = connect(f);
  const { done } = await begin(f);
  session.select(image);
  await tick();
  await f.channel.flush();
  session.retire();
  await done;
  for (const denied of [false, true]) {
    const cold = fixture(f.durable());
    const port = photoPort();
    port.load = async () => [];
    port.refresh = async () => [];
    if (denied)
      port.permission = async () => ({
        granted: false,
        status: 'denied' as PhotoLibraryPermission['status'],
        canAskAgain: false,
        accessPrivileges: 'none',
      });
    port.materialize = async () => {
      throw Error('must never resolve original Photos id');
    };
    const restored = connect(cold, port);
    const reopened = await begin(cold);
    await tick();
    assert.deepEqual(
      restored.getSnapshot().selected.map((item) => item.id),
      [image.id],
    );
    assert.ok(restored.getSnapshot().previewUris[image.id]);
    assert.equal(restored.canApprove(), true);
    restored.cancel();
    await reopened.done;
    assert.equal(cold.sent.length, 0);
  }
});

test('late durable preview lease after account retirement never appears in the old or replacement session', async () => {
  const f = fixture();
  let release!: () => void;
  f.gateMaterialize(
    new Promise<void>((yes) => {
      release = yes;
    }),
  );
  const session = connect(f);
  const { done } = await begin(f);
  session.select(image);
  await tick();
  await f.channel.flush();
  await tick();
  assert.equal(session.getSnapshot().previewUris[image.id], null);
  f.retire();
  session.retire();
  release();
  await done;
  await tick();
  assert.deepEqual(session.getSnapshot().previewUris, {});
  assert.equal(f.sent.length, 0);
  assert.ok(
    f.released.some((uri) => uri.startsWith('file:///cache/fresh-')),
    'closed channel releases its late materialization',
  );
});
