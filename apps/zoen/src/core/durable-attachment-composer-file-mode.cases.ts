import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import {
  deferred,
  findStrip,
  fixture,
  photo,
  tick,
} from './durable-attachment-composer.fixtures.ts';

test('Photos Send as Files waits for removal/save and journals the exact file mode, captions and ordered identities', async () => {
  const f = fixture(),
    removed = deferred<AttachmentAsset[]>();
  f.render();
  await tick();
  f.select(() => removed.promise);
  f.submitPhotos({
    caption: 'File A 🪁',
    captions: ['File A 🪁', 'File B'],
    send: true,
    options: { presentation: 'file', grouping: 'separate' },
  });
  const picking = f.render().pick('photo');
  await tick();
  assert.equal(f.photoAdmissions.at(-1)?.sendAsFileAllowed, true);
  assert.deepEqual(f.journalEvents, []);
  assert.equal(f.sendCalls(), 0);
  removed.resolve([photo('first.jpg'), { ...photo('second.mov'), mime: 'video/quicktime' }]);
  await picking;
  const sent = JSON.parse(f.sent[0]!);
  assert.deepEqual(sent.options, { presentation: 'file', grouping: 'separate' });
  assert.deepEqual(
    sent.items.map((value: { caption: string }) => value.caption),
    ['File A 🪁', 'File B'],
  );
  assert.deepEqual(f.journalEvents, ['save-start', 'save-accepted', 'send']);
  assert.equal(f.sent[0], f.writes[0]);
  assert.equal(f.sendCalls(), 1);
  // The same owner starts a fresh ordinary selection after successful delivery.
  f.select([photo('ordinary.jpg')]);
  await f.render().pick('photo');
  assert.equal(JSON.parse(f.writes.at(-1)!).options, undefined);
  assert.equal(f.sendCalls(), 1);
  f.unmount();
});

test('an existing ordinary-media draft cannot be converted by a forged file picker submission', async () => {
  const content = {
    kind: 'attachments',
    reply_to: null,
    items: [{ asset_id: 'old', mutation_id: 'old-send', caption: 'Keep ordinary' }],
  };
  const initial = {
    revision: 'saved',
    content_json: JSON.stringify(content),
    attempted: false,
    assets: [{ id: 'old', name: 'old.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) }],
  };
  const f = fixture(initial);
  f.render();
  await tick();
  f.select([photo('new.jpg')]);
  f.submitPhotos({
    caption: 'Attempt conversion',
    send: true,
    options: { presentation: 'file', grouping: 'separate' },
  });
  await f.render().pick('photo');
  assert.equal(f.photoAdmissions.at(-1)?.sendAsFileAllowed, false);
  assert.deepEqual(f.journalEvents, []);
  assert.equal(f.sendCalls(), 0);
  assert.equal(f.durable().content_json, initial.content_json);
  f.unmount();
});

test('a file-mode picker cannot commit against a changed canonical revision even if the draft is still empty', async () => {
  const f = fixture(),
    removed = deferred<AttachmentAsset[]>();
  f.render();
  await tick();
  f.select(() => removed.promise);
  f.submitPhotos({
    caption: 'Stale mode',
    send: true,
    options: { presentation: 'file', grouping: 'separate' },
  });
  const picking = f.render().pick('photo');
  await tick();
  await f.channel.save(null, []);
  const writesBeforeReturn = f.writes.length;
  removed.resolve([photo()]);
  await picking;
  assert.equal(f.writes.length, writesBeforeReturn);
  assert.equal(f.sendCalls(), 0);
  assert.equal(f.durable().content_json, 'null');
  f.unmount();
});

test('restored file-mode drafts keep their options and old captions through append, reorder and explicit Send', async () => {
  const content = {
    kind: 'attachments',
    reply_to: 'old-reply',
    options: { presentation: 'file', grouping: 'separate' },
    items: [{ asset_id: 'old', mutation_id: 'old-send', caption: 'Old caption' }],
  };
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify(content),
    attempted: false,
    assets: [{ id: 'old', name: 'old.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) }],
  });
  f.render();
  await tick();
  f.select([photo('new.jpg')]);
  f.submitPhotos({ caption: 'New caption', send: false });
  await f.render().pick('photo');
  assert.equal(f.photoAdmissions.at(-1)?.sendAsFileAllowed, true);
  let approval = f.approval()!;
  assert.ok(approval);
  approval.onReorder!([...approval.state.items].reverse().map((item) => item.mutationId));
  await tick();
  const reordered = JSON.parse(f.durable().content_json);
  assert.deepEqual(reordered.options, content.options);
  assert.deepEqual(
    reordered.items.map((item: { caption: string }) => item.caption),
    ['New caption', 'Old caption'],
  );
  assert.equal(f.sendCalls(), 0);
  approval = f.approval()!;
  approval.field.onSend();
  await tick();
  assert.deepEqual(JSON.parse(f.sent[0]!).options, content.options);
  f.unmount();
});

test('a canceled file selection does not carry its options into a later ordinary document selection', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([]);
  f.submitPhotos({
    caption: 'Canceled',
    send: true,
    options: { presentation: 'file', grouping: 'separate' },
  });
  await f.render().pick('photo');
  assert.deepEqual(f.journalEvents, []);
  f.select([{ ...photo('next.pdf'), mime: 'application/pdf' }]);
  await f.render().pick('document');
  const saved = JSON.parse(f.durable().content_json);
  assert.equal(saved.options, undefined);
  assert.equal(saved.items[0].caption, '');
  assert.equal(f.sendCalls(), 0);
  f.unmount();
});

test('a rejected file-mode journal save can be recovered and discarded before a fresh ordinary selection', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('rejected.jpg')]);
  f.submitPhotos({
    caption: 'Never accepted',
    send: true,
    options: { presentation: 'file', grouping: 'separate' },
  });
  f.saveFailure(true);
  await f.render().pick('photo');
  assert.equal(f.durable().content_json, 'null');
  assert.equal(f.sendCalls(), 0);
  f.saveFailure(false);
  // A rejected save retains its owned pending bytes. The existing explicit
  // retry/discard controls resolve that draft before a fresh selection.
  await f.channel.retry();
  assert.equal(JSON.parse(f.durable().content_json).options.presentation, 'file');
  assert.equal(f.sendCalls(), 0);
  (findStrip(f.render().review).props.onCancel as () => void)();
  await f.channel.flush();
  await tick();
  assert.equal(f.durable().content_json, 'null');
  f.select([photo('ordinary.jpg')]);
  await f.render().pick('photo');
  const saved = JSON.parse(f.durable().content_json);
  assert.equal(saved.options, undefined);
  assert.equal(saved.items[0].caption, '');
  assert.equal(f.sendCalls(), 0);
  f.unmount();
});

test('an ambiguous as-file Send retries the accepted mode and receipt without duplicate messages', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('kept.heic')]);
  f.submitPhotos({
    caption: 'Exact accepted file intent',
    send: true,
    options: { presentation: 'file', grouping: 'separate' },
  });
  f.loseNextSendAck();
  await f.render().pick('photo');
  assert.equal(f.sendCalls(), 1);
  assert.equal(f.sent.length, 1);
  assert.deepEqual(JSON.parse(f.sent[0]!).options, { presentation: 'file', grouping: 'separate' });
  assert.equal(f.channel.getSnapshot().frozen, true);
  // Direct Photos Send has already closed its picker; its compact draft field
  // owns the existing retry rather than opening an unrelated approval surface.
  const retry = f.render().field;
  assert.ok(retry);
  retry.onSend();
  await tick();
  assert.equal(f.sendCalls(), 2);
  assert.equal(f.sent.length, 1);
  assert.equal(f.render().active, false);
  f.unmount();
});
