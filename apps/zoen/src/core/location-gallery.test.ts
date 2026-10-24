import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, findStrip, scope, tick } from './durable-attachment-composer.fixtures.ts';

test('cold mixed attachments open the static location in the existing Gallery and Close preserves exact canonical content without QuickLook/send', async () => {
  const items = [
    { asset_id: 'photo', mutation_id: 'photo-post', caption: 'Photo caption' },
    { asset_id: 'point', mutation_id: 'point-post', caption: 'Location caption' },
    { asset_id: 'video', mutation_id: 'video-post', caption: 'Video caption' },
  ];
  const initial = {
    revision: 'saved',
    content_json: JSON.stringify({
      kind: 'attachments',
      reply_to: 'original-reply',
      options: { presentation: 'file', grouping: 'separate' },
      items,
    }),
    assets: [
      { id: 'photo', name: 'photo.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) },
      {
        id: 'point',
        name: 'location.geojson',
        mime: 'application/geo+json',
        size: 120,
        sha256: 'b'.repeat(64),
      },
      { id: 'video', name: 'video.mp4', mime: 'video/mp4', size: 12, sha256: 'c'.repeat(64) },
    ],
    attempted: false,
  };
  const f = fixture(initial, { ...scope, thread_root_id: 'original-thread' });
  f.render();
  await tick();
  f.render();
  await tick();
  const before = f.durable().content_json;
  (findStrip(f.render().review).props.onPreview as (id: string) => void)('point-post');
  const approval = f.approval();
  assert.ok(approval, 'GeoJSON review must reuse the mounted Gallery instead of QuickLook');
  assert.equal(f.previews.length, 0);
  assert.equal(approval.field.scope.replyTo, 'original-reply');
  assert.equal(approval.field.scope.thread, true);
  assert.deepEqual(
    approval.state.items.map((item) => item.mutationId),
    ['photo-post', 'point-post', 'video-post'],
  );
  assert.equal(approval.selectedId, 'point-post');
  assert.equal(approval.state.items[1]!.sha256, 'b'.repeat(64));
  const staleSend = approval.field.onSend;
  approval.onClose();
  await tick();
  staleSend();
  await tick();
  assert.equal(f.durable().content_json, before);
  assert.equal(f.writes.length, 0);
  assert.equal(f.sendCalls(), 0);
});
