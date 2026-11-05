import assert from 'node:assert/strict';
import test from 'node:test';
import {
  fixture,
  empty,
  scope,
  photo,
  tick,
  deferred,
} from './durable-attachment-composer.fixtures.ts';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
const editScope = { ...scope, edit_message_id: 'message' };
const mediaEdit = { id: 'message', revision: 'signed-source', text: 'Existing caption' };

test('mounted editor uses the existing photo picker with one item and the current caption', async () => {
  const f = fixture(empty(), editScope);
  f.render({ mediaEdit });
  await tick();
  f.nativePhotoPicker(async (maximum, current, admission) => {
    assert.equal(maximum, 1);
    assert.equal(current(), true);
    assert.equal(admission?.caption, 'Existing caption');
    assert.equal(admission?.sendAsFileAllowed, false);
    return [photo('replacement.jpg')];
  });
  await f.render().pick('photo');
  const selected = JSON.parse(f.durable().content_json);
  assert.equal(selected.edit_revision, 'signed-source');
  assert.equal(selected.reply_to, null);
  assert.equal(selected.items.length, 1);
  assert.equal(selected.items[0].caption, 'Existing caption');
  assert.equal(f.sent.length, 0);
  assert.equal(f.render().field?.value, 'Existing caption');
  f.unmount();
});
test('replacement picker callback after retirement releases only its selected copy', async () => {
  const f = fixture(empty(), editScope),
    wait = deferred<AttachmentAsset[]>();
  f.render({ mediaEdit });
  await tick();
  f.nativePhotoPicker(async () => wait.promise);
  const picking = f.render().pick('photo');
  f.unmount();
  wait.resolve([photo('late.jpg')]);
  await picking;
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.sent.length, 0);
  assert.ok(f.discarded.includes(photo('late.jpg').uri));
});

test('cancel editing waits for the private journal clear before dismissing the normal editor', async () => {
  const f = fixture(empty(), editScope);
  f.render({ mediaEdit });
  await tick();
  f.select([photo('replacement.jpg')]);
  await f.render().pick('photo');
  const gate = deferred<void>();
  f.holdSaves(gate.promise);
  let settled = false;
  const cancelling = f
    .render()
    .cancelEditMedia?.()
    .then((cleared) => {
      settled = true;
      return cleared;
    });
  await tick();
  assert.equal(settled, false);
  assert.notEqual(f.durable().content_json, 'null');
  gate.resolve();
  assert.equal(await cancelling, true);
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.sent.length, 0);
  f.unmount();
});
