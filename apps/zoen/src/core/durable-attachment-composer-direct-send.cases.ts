import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import { deferred, fixture, photo, tick } from './durable-attachment-composer.fixtures.ts';

test('Photos gallery Send preserves captions in the exact ordered selection without using filenames as identity', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('same.jpg'), { ...photo('same.jpg'), uri: 'file:///cache/other.jpg' }]);
  f.submitPhotos({ caption: 'First', captions: ['First', 'Second 🪁'], send: true });
  await f.render().pick('photo');
  assert.equal(f.sendCalls(), 1);
  const sent = JSON.parse(f.sent[0]!);
  assert.deepEqual(
    sent.items.map((value: { caption: string }) => value.caption),
    ['First', 'Second 🪁'],
  );
  assert.notEqual(sent.items[0].mutation_id, sent.items[1].mutation_id);
});

test('Photos direct Send cannot bypass a rejected caption and attachment journal save', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  f.submitPhotos({ caption: 'Must be durable before Send', send: true });
  f.saveFailure(true);
  await f.render().pick('photo');
  assert.deepEqual(f.journalEvents, ['save-start']);
  assert.deepEqual(f.writes, []);
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.sendCalls(), 0);
  assert.equal(f.approval(), undefined);
  f.unmount();
});

test('Photos direct Send preserves a recovered batch and applies its caption only to the first new asset', async () => {
  const content = {
    kind: 'attachments',
    reply_to: 'original-reply',
    items: [{ asset_id: 'old', mutation_id: 'old-send', caption: 'Existing caption' }],
  };
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify(content),
    attempted: false,
    assets: [
      {
        id: 'old',
        name: 'old.jpg',
        mime: 'image/jpeg',
        size: 12,
        sha256: 'a'.repeat(64),
      },
    ],
  });
  f.render();
  await tick();
  f.select([photo('first-new.jpg'), photo('second-new.jpg')]);
  f.submitPhotos({ caption: 'New selection caption', send: true });
  await f.render({ replyTo: 'later-reply' }).pick('photo');
  assert.equal(f.sendCalls(), 1);
  const sent = JSON.parse(f.sent[0]!);
  assert.equal(sent.reply_to, 'original-reply');
  assert.deepEqual(
    sent.items.map((value: { caption: string }) => value.caption),
    ['Existing caption', 'New selection caption', ''],
  );
  assert.equal(sent.items[0].asset_id, 'old');
  assert.equal(sent.items[0].mutation_id, 'old-send');
  assert.equal(f.approval(), undefined);
});

test('Photos direct Send is revoked when the picker returns after account or destination retirement', async () => {
  for (const retirement of ['retire', 'destination'] as const) {
    const f = fixture(),
      removed = deferred<AttachmentAsset[]>();
    f.render();
    await tick();
    f.select(() => removed.promise);
    f.submitPhotos({ caption: 'Retired intent', send: true });
    const picking = f.render().pick('photo');
    await tick();
    if (retirement === 'destination') f.render({ spaceId: 'other-chat' });
    else f[retirement]();
    removed.resolve([photo()]);
    await picking;
    assert.deepEqual(f.journalEvents, [], retirement);
    assert.equal(f.sendCalls(), 0, retirement);
    assert.equal(f.approval(), undefined, retirement);
    assert.deepEqual(f.discarded, [photo().uri], retirement);
    f.unmount();
  }
});

test('account, destination or route revocation while a direct Send save is pending cannot queue delivery', async () => {
  for (const retirement of ['retire', 'destination', 'unmount'] as const) {
    const f = fixture(),
      saved = deferred<void>();
    f.render();
    await tick();
    f.select([photo()]);
    f.submitPhotos({ caption: 'Keep the saved draft, revoke Send', send: true });
    f.holdSaves(saved.promise);
    const picking = f.render().pick('photo');
    await tick();
    assert.deepEqual(f.journalEvents, ['save-start']);
    if (retirement === 'destination') f.render({ spaceId: 'other-chat' });
    else f[retirement]();
    saved.resolve();
    await picking;
    assert.equal(f.sendCalls(), 0, retirement);
    assert.equal(f.approval(), undefined, retirement);
    assert.equal(
      JSON.parse(f.durable().content_json).items[0].caption,
      'Keep the saved draft, revoke Send',
    );
    f.unmount();
  }
});

test('an explicit Send accepted in foreground completes exactly once when only the app backgrounds during its durable save', async () => {
  const f = fixture(),
    saved = deferred<void>();
  f.render();
  await tick();
  f.select([photo()]);
  f.submitPhotos({ caption: 'Already authorized delivery', send: true });
  f.holdSaves(saved.promise);
  const picking = f.render().pick('photo');
  await tick();
  assert.deepEqual(f.journalEvents, ['save-start']);
  // Background alone does not revoke this already explicit Send. Identity,
  // destination and route lifetime still fence it in the separate regression.
  f.background();
  saved.resolve();
  await picking;
  assert.deepEqual(f.journalEvents, ['save-start', 'save-accepted', 'send']);
  assert.equal(f.sendCalls(), 1);
  assert.equal(f.sent[0], f.writes[0]);
  const accepted = JSON.parse(f.sent[0]!);
  assert.equal(accepted.items[0].caption, 'Already authorized delivery');
  assert.equal(accepted.items[0].mutation_id, 'asset-1');
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.approval(), undefined);
  f.unmount();
});

test('Photos Review persists its caption and opens approval without sending', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  f.submitPhotos({ caption: 'Review before sending', send: false });
  await f.render().pick('photo');
  const approval = f.approval();
  assert.ok(approval);
  assert.equal(approval.field.value, 'Review before sending');
  assert.equal(JSON.parse(f.durable().content_json).items[0].caption, 'Review before sending');
  assert.equal(f.sendCalls(), 0);
  assert.deepEqual(f.journalEvents, ['save-start', 'save-accepted']);
  f.unmount();
});

test('an empty Photos result cannot send and its stale submission cannot authorize a later document pick', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([]);
  f.submitPhotos({ caption: 'Stale Photos Send', send: true });
  await f.render().pick('photo');
  assert.deepEqual(f.journalEvents, []);
  assert.equal(f.sendCalls(), 0);
  f.select([{ ...photo('file.pdf'), mime: 'application/pdf' }]);
  await f.render().pick('document');
  assert.equal(JSON.parse(f.durable().content_json).items[0].caption, '');
  assert.equal(f.sendCalls(), 0);
  assert.deepEqual(f.journalEvents, ['save-start', 'save-accepted']);
  f.unmount();
});
