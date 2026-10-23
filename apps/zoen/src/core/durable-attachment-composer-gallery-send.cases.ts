import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, photo, tick } from './durable-attachment-composer.fixtures.ts';

test('native gallery Send journals its final keystroke after older queued caption saves and coalesces a duplicate tap', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  const gate = deferred<void>();
  f.holdSaves(gate.promise);
  const ui = f.render();
  ui.field!.onChange('Older native callback');
  ui.field!.onSend('  Final 🪁 keystroke\nexact  ');
  ui.field!.onSend('Duplicate must not replace the accepted intent');
  await tick();
  assert.equal(f.sendCalls(), 0);
  gate.resolve();
  await f.channel.flush();
  await tick();
  assert.equal(f.sendCalls(), 1);
  assert.equal(JSON.parse(f.sent[0]!).items[0].caption, '  Final 🪁 keystroke\nexact  ');
});

test(
  'Photos Edit stages the authorized selection and final caption before opening the canonical editor, without sending',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    f.submitPhotos({
      caption: 'Before editing 🪁',
      captions: ['Before editing 🪁'],
      send: false,
      requestEdit: true,
    });
    const picking = f.render().pick('photo');
    await f.previewStarted;
    assert.equal(f.sendCalls(), 0);
    assert.equal(JSON.parse(f.durable().content_json).items[0].caption, 'Before editing 🪁');
    assert.equal(f.channel.getSnapshot().reviewing, true);
    f.saveEditedCopy();
    await picking;
    const saved = JSON.parse(f.durable().content_json);
    assert.equal(saved.items[0].asset_id, 'edited-asset-1');
    assert.equal(saved.items[0].caption, 'Before editing 🪁');
    assert.equal(f.sendCalls(), 0);
  },
);

test('native selected-media reordering preserves the latest captions, identities and captured destination', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('first.jpg'), photo('second.jpg')]);
  await f.render({ replyTo: 'captured-thread' }).pick('photo');
  const approval = f.approval()!;
  const ids = approval.state.items.map((item) => item.mutationId);
  approval.field.onChange('Caption newer than the rendered reorder action');
  approval.onReorder?.([...ids].reverse());
  await f.channel.flush();
  const saved = JSON.parse(f.durable().content_json);
  assert.deepEqual(
    saved.items.map((item: { mutation_id: string }) => item.mutation_id),
    [...ids].reverse(),
  );
  assert.deepEqual(
    saved.items.map((item: { caption: string }) => item.caption),
    ['', 'Caption newer than the rendered reorder action'],
  );
  assert.equal(saved.reply_to, 'captured-thread');
  const previous = f.durable().content_json;
  approval.onReorder?.([ids[0]!, ids[0]!]);
  approval.onReorder?.(['foreign', ids[1]!]);
  await tick();
  assert.equal(f.durable().content_json, previous);
});

test('native gallery final-caption save failure cannot send or discard the previous durable revision', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  const previous = f.durable().content_json;
  f.saveFailure(true);
  f.render().field!.onSend('Do not deliver without a durable caption');
  await tick();
  assert.equal(f.sendCalls(), 0);
  assert.equal(f.durable().content_json, previous);
  assert.equal(f.render().active, true);
});

test('native gallery final-caption Send is revoked by a destination change while its journal is saving', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  const gate = deferred<void>();
  f.holdSaves(gate.promise);
  f.render().field!.onSend('Retain this draft, revoke delivery');
  await tick();
  f.render({ spaceId: 'other-space' });
  gate.resolve();
  await f.channel.flush();
  await tick();
  assert.equal(f.sendCalls(), 0);
  assert.equal(
    JSON.parse(f.durable().content_json).items[0].caption,
    'Retain this draft, revoke delivery',
  );
});

test(
  'explicit retry after an ambiguous native Send uses the same receipt without duplicating its message',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    f.loseNextSendAck();
    f.render().field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 1);
    assert.equal(f.sent.length, 1);
    assert.equal(f.channel.getSnapshot().frozen, true);
    const retry = f.approval()!;
    assert.equal(retry.field.retry, true, 'Ambiguous Send still offers its exact receipt retry');
    assert.equal(retry.state.items[0]!.attempted, true);
    retry.field.onSend();
    await tick();
    assert.equal(f.sendCalls(), 2);
    assert.equal(f.sent.length, 1);
    assert.equal(f.render().active, false);
  },
);
