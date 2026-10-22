import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import { deferred, fixture, photo, tick } from './durable-attachment-composer.fixtures.ts';

test('Photos album journals exact grouped order and captions before the canonical Send and replays its accepted receipt', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('b.jpg'), photo('a.jpg')]);
  f.submitPhotos({
    caption: 'B first',
    captions: ['B first', 'A second'],
    send: true,
    options: { presentation: 'media', grouping: 'grouped' },
  });
  f.loseNextSendAck();
  await f.render().pick('photo');
  assert.deepEqual(f.journalEvents, ['save-start', 'save-accepted', 'send']);
  const accepted = JSON.parse(f.sent[0]!);
  assert.deepEqual(accepted.options, { presentation: 'media', grouping: 'grouped' });
  assert.deepEqual(
    accepted.items.map((item: { caption: string }) => item.caption),
    ['B first', 'A second'],
  );
  assert.equal(f.sent[0], f.writes[0]);
  assert.equal(f.channel.getSnapshot().frozen, true);
  f.render().field!.onSend();
  await tick();
  assert.equal(f.sendCalls(), 2);
  assert.equal(f.sent.length, 1);
  f.unmount();
});

test('album admission rejects a changed draft revision or a singleton grouped selection without saving or sending', async () => {
  for (const changed of [false, true]) {
    const f = fixture(),
      returned = deferred<AttachmentAsset[]>();
    f.render();
    await tick();
    f.select(() => returned.promise);
    f.submitPhotos({
      caption: 'Stale album',
      send: true,
      options: { presentation: 'media', grouping: 'grouped' },
    });
    const picking = f.render().pick('photo');
    await tick();
    if (changed) await f.channel.save(null, []);
    const before = f.writes.length;
    returned.resolve(changed ? [photo('a.jpg'), photo('b.jpg')] : [photo('one.jpg')]);
    await picking;
    assert.equal(f.writes.length, before);
    assert.equal(f.sendCalls(), 0);
    assert.equal(f.durable().content_json, 'null');
    f.unmount();
  }
});

test('recovered grouped drafts preserve admitted mode on append/reorder and explicitly become separate after removing down to one', async () => {
  const content = {
    kind: 'attachments',
    reply_to: null,
    options: { presentation: 'media', grouping: 'grouped' },
    items: [
      { asset_id: 'old-a', mutation_id: 'old-send-a', caption: 'A' },
      { asset_id: 'old-b', mutation_id: 'old-send-b', caption: 'B' },
    ],
  };
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify(content),
    attempted: false,
    assets: ['a', 'b'].map((id) => ({
      id: `old-${id}`,
      name: `${id}.jpg`,
      mime: 'image/jpeg',
      size: 12,
      sha256: 'a'.repeat(64),
    })),
  });
  f.render();
  await tick();
  f.select([photo('c.jpg')]);
  f.submitPhotos({
    caption: 'C',
    send: false,
    options: { presentation: 'media', grouping: 'grouped' },
  });
  await f.render().pick('photo');
  assert.deepEqual(f.photoAdmissions.at(-1)?.existingOptions, content.options);
  let approval = f.approval()!;
  assert.equal(approval.grouped, true);
  approval.onReorder!([...approval.state.items].reverse().map((item) => item.mutationId));
  await tick();
  assert.deepEqual(
    JSON.parse(f.durable().content_json).items.map((item: { caption: string }) => item.caption),
    ['C', 'B', 'A'],
  );
  approval = f.approval()!;
  approval.onRemove(approval.state.items[0]!.mutationId);
  await tick();
  assert.equal(JSON.parse(f.durable().content_json).options.grouping, 'grouped');
  approval = f.approval()!;
  approval.onRemove(approval.state.items[0]!.mutationId);
  await tick();
  const remaining = JSON.parse(f.durable().content_json);
  assert.equal(remaining.items.length, 1);
  assert.equal(remaining.items[0].caption, 'A');
  assert.equal(
    remaining.options,
    undefined,
    'the canonical serializer retains the legacy separate-media envelope',
  );
  assert.equal(f.sendCalls(), 0);
  f.unmount();
});
