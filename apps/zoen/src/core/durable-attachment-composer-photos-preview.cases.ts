import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import { deferred, fixture, photo, tick } from './durable-attachment-composer.fixtures.ts';

test('library preview focus selects the exact returned copy without reordering the durable batch or changing Send', async () => {
  const f = fixture();
  f.render();
  await tick();
  const picked = [photo('first.jpg'), photo('viewed.jpg')];
  f.select(picked);
  f.preferPhoto(picked[1]!.uri);
  await f.render().pick('photo');
  const approval = f.approval()!;
  assert.deepEqual(
    approval.state.items.map((item) => item.name),
    ['first.jpg', 'viewed.jpg'],
  );
  assert.equal(approval.selectedId, approval.state.items[1]!.mutationId);
  assert.equal(approval.field.position, 2);
  assert.equal(f.sendCalls(), 0);
  assert.equal(approval.state.items[0]!.caption, '');
});

test('Photos direct Send waits for picker removal and durably saves the exact caption before sending once', async () => {
  const f = fixture(),
    removed = deferred<AttachmentAsset[]>();
  f.render();
  await tick();
  f.select(() => removed.promise);
  f.submitPhotos({ caption: '  Final keystroke! 🖼️\nkept  ', send: true });
  const picking = f.render({ replyTo: 'captured-reply' }).pick('photo');
  await tick();
  assert.deepEqual(f.journalEvents, []);
  assert.equal(f.approval(), undefined);
  assert.equal(f.sendCalls(), 0);
  removed.resolve([photo('first.jpg'), photo('second.jpg')]);
  await picking;
  assert.deepEqual(f.journalEvents, ['save-start', 'save-accepted', 'send']);
  assert.equal(f.sendCalls(), 1);
  assert.equal(f.sent.length, 1);
  const sent = JSON.parse(f.sent[0]!);
  assert.equal(sent.reply_to, 'captured-reply');
  assert.deepEqual(
    sent.items.map((value: { caption: string }) => value.caption),
    ['  Final keystroke! 🖼️\nkept  ', ''],
  );
  assert.equal(f.sent[0], f.writes[0], 'Send must consume the exact accepted draft revision');
  assert.equal(f.approval(), undefined, 'Direct Send must not briefly enter media approval');
  assert.equal(f.durable().content_json, 'null');
});
