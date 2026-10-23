import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import {
  deferred,
  find,
  findStrip,
  fixture,
  photo,
  tick,
} from './durable-attachment-composer.fixtures.ts';

test('compiled attachment hook retains batch order, each caption, and the original null reply when the ordinary draft changes', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('one.jpg'), photo('two.jpg')]);
  await f.render().pick('photo');
  let ui = f.render({ replyTo: 'later-reply' });
  assert.equal(ui.field?.scope.replyTo, null);
  ui.field!.onChange('First caption');
  await f.channel.flush();
  ui = f.render();
  assert.equal(ui.field?.value, 'First caption');
  const strip = findStrip(ui.review);
  (strip.props.onSelect as (id: string) => void)('asset-2');
  ui = f.render();
  ui.field!.onChange('Second caption');
  await f.channel.flush();
  const content = JSON.parse(f.durable().content_json);
  assert.deepEqual(
    content.items.map((item: { caption: string }) => item.caption),
    ['First caption', 'Second caption'],
  );
  ui = f.render();
  ui.field!.onSend();
  await tick();
  assert.equal(f.sent.length, 1);
  assert.equal(JSON.parse(f.sent[0]!).reply_to, null);
  assert.equal(f.render().active, false);
  await f.channel.flush();
  assert.deepEqual(f.discarded.sort(), [photo('one.jpg').uri, photo('two.jpg').uri]);
});

test('compiled attachment hook restores encrypted metadata through fresh preview leases and retries a failed materialization', async () => {
  const content = {
    kind: 'attachments',
    reply_to: 'saved-reply',
    items: [{ asset_id: 'old', caption: 'Still here', mutation_id: 'old' }],
  };
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify(content),
    attempted: false,
    assets: [{ id: 'old', name: 'old.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) }],
  });
  f.previewFailure(true);
  f.render();
  await tick();
  f.render();
  await tick();
  let ui = f.render();
  assert.equal(ui.active, true);
  assert.equal(ui.field?.editable, false);
  f.previewFailure(false);
  (find(ui.review, 'retry-media-draft').props.onPress as () => void)();
  await tick();
  f.render();
  await tick();
  ui = f.render();
  assert.equal(ui.field?.editable, true);
  assert.equal(ui.field?.value, 'Still here');
  assert.equal(ui.field?.scope.replyTo, 'saved-reply');
  f.unmount();
  await f.channel.flush();
  assert.deepEqual(f.released, ['file:///cache/fresh-old']);
  assert.equal(JSON.parse(f.durable().content_json).items[0].caption, 'Still here');
});

test('picker completion after route unmount discards only its owned copies and creates no draft', async () => {
  const f = fixture();
  f.render();
  await tick();
  const wait = deferred<AttachmentAsset[]>();
  f.select(() => wait.promise);
  const picking = f.render().pick('photo');
  f.unmount();
  wait.resolve([photo()]);
  await picking;
  await f.channel.flush();
  assert.deepEqual(f.nativeReleased, [photo().uri]);
  assert.deepEqual(f.discarded, [photo().uri]);
  assert.equal(f.writes.length, 0);
});

test('an unread durable draft keeps attachment admission closed until an explicit successful retry', async () => {
  const f = fixture();
  f.readFailure(true);
  f.render();
  await tick();
  let ui = f.render();
  assert.equal(ui.canAdd, false);
  f.select([photo()]);
  await ui.pick('photo');
  assert.equal(f.writes.length, 0);
  f.readFailure(false);
  (find(ui.review, 'retry-media-draft').props.onPress as () => void)();
  await tick();
  ui = f.render();
  assert.equal(ui.canAdd, true);
  await ui.pick('photo');
  assert.equal(f.writes.length, 1);
});

test('a previously rendered Remove action preserves the latest caption before React commits', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('one.jpg'), photo('two.jpg')]);
  await f.render().pick('photo');
  const ui = f.render();
  const strip = findStrip(ui.review);
  ui.field!.onChange('Latest caption before commit');
  (strip.props.onRemove as (id: string) => void)('asset-2');
  await tick();
  await f.channel.flush();
  const content = JSON.parse(f.durable().content_json);
  assert.equal(content.items.length, 1);
  assert.equal(content.items[0].caption, 'Latest caption before commit');
});

test(
  'pre-send review opens the selected saved asset and preserves captions and inline destination',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo('one.jpg'), { ...photo('report.pdf'), mime: 'application/pdf' }]);
    await f.render({ replyTo: 'inline-root' }).pick('document');
    let ui = f.render({ replyTo: 'later-root' });
    ui.field!.onChange('  Caption 🗂️\nkept exactly  ');
    await f.channel.flush();
    ui = f.render();
    const strip = findStrip(ui.review);
    (strip.props.onSelect as (id: string) => void)('asset-2');
    ui = f.render();
    ui.field!.onChange('PDF caption');
    const second = findStrip(ui.review);
    (second.props.onPreview as (id: string) => void)('asset-2');
    await f.previewStarted;
    assert.equal(f.previews.length, 1);
    assert.equal(f.previews[0]!.assetId, 'asset-2');
    assert.equal(f.previews[0]!.revision, f.durable().revision);
    assert.equal(f.render().busy, true);
    const content = JSON.parse(f.durable().content_json);
    assert.equal(content.reply_to, 'inline-root');
    assert.deepEqual(
      content.items.map((item: { caption: string }) => item.caption),
      ['  Caption 🗂️\nkept exactly  ', 'PDF caption'],
    );
    assert.deepEqual(f.sent, [], 'review must never send');
    f.closePreview();
    await tick();
    ui = f.render();
    assert.equal(ui.busy, false);
    assert.equal(ui.field?.value, 'PDF caption');
    assert.equal(ui.field?.scope.replyTo, 'inline-root');
  },
);

test('backgrounding, destination replacement and session revocation cancel review and preserve the durable draft', async () => {
  for (const revoke of ['background', 'retire', 'destination'] as const) {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    f.render();
    f.openNativePreview('asset-1');
    await tick();
    if (revoke === 'destination') f.render({ spaceId: 'another-chat' });
    else f[revoke]();
    await tick();
    assert.deepEqual(f.canceled, ['presentation-1']);
    assert.equal(JSON.parse(f.durable().content_json).items[0].asset_id, 'asset-1');
    assert.deepEqual(f.sent, []);
  }
});

test('a stale Remove action dismisses review before dropping the selected draft asset', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo('one.jpg'), photo('two.jpg')]);
  await f.render().pick('photo');
  const strip = findStrip(f.render().review);
  f.openNativePreview('asset-1');
  await tick();
  (strip.props.onRemove as (id: string) => void)('asset-1');
  await tick();
  await f.channel.flush();
  f.render();
  await f.channel.flush();
  assert.deepEqual(f.canceled, ['presentation-1']);
  assert.deepEqual(
    JSON.parse(f.durable().content_json).items.map((item: { asset_id: string }) => item.asset_id),
    ['asset-2'],
  );
  assert.deepEqual(f.discarded, [photo('one.jpg').uri]);
  assert.deepEqual(f.sent, []);
});

test('unmount and device cleanup coalesce cancellation and preserve the encrypted selection', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  f.openNativePreview('asset-1');
  await tick();
  f.unmount();
  await f.erase();
  await f.channel.flush();
  assert.deepEqual(f.canceled, ['presentation-1']);
  assert.equal(JSON.parse(f.durable().content_json).items[0].asset_id, 'asset-1');
  assert.deepEqual(f.discarded, [photo().uri]);
});

test(
  'a stale Send callback during native edited-copy review cannot dismiss it or send the later edited draft',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render({ replyTo: 'reviewed-reply' }).pick('photo');
    let ui = f.render();
    ui.field!.onChange('  Reviewed caption 🪁\nexact bytes  ');
    await f.channel.flush();
    ui = f.render();
    const staleSend = ui.field!.onSend;
    f.openNativePreview('asset-1');
    await f.previewStarted;
    assert.equal(f.channel.getSnapshot().reviewing, true);
    staleSend();
    await tick();
    assert.deepEqual(f.canceled, [], 'Send must not request native dismissal');
    assert.equal(f.sendCalls(), 0, 'Send must not be queued behind the pending editor');
    assert.equal(f.channel.getSnapshot().reviewing, true);
    f.saveEditedCopy();
    await tick();
    f.render();
    await tick();
    ui = f.render();
    assert.equal(ui.busy, false);
    assert.equal(f.sendCalls(), 0, 'Native Save must not awaken the stale Send callback');
    const edited = JSON.parse(f.durable().content_json);
    assert.equal(edited.items[0].asset_id, 'edited-asset-1');
    assert.equal(edited.items[0].mutation_id, 'post-edited-asset-1');
    assert.equal(edited.items[0].caption, '  Reviewed caption 🪁\nexact bytes  ');
    assert.equal(edited.reply_to, 'reviewed-reply');
    assert.equal(f.channel.getSnapshot().value!.assets[0]!.sha256, 'b'.repeat(64));
    staleSend();
    await tick();
    assert.equal(
      f.sendCalls(),
      0,
      'The old callback cannot target the replacement after dismissal',
    );
    ui.field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 1);
    assert.deepEqual(JSON.parse(f.sent[0]!), edited);
    assert.equal(f.render().active, false);
  },
);

test(
  'a stale Send callback during read-only native review leaves the viewer and draft untouched',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([{ ...photo('report.pdf'), mime: 'application/pdf' }]);
    await f.render().pick('document');
    const ui = f.render(),
      staleSend = ui.field!.onSend;
    f.openNativePreview('asset-1');
    await f.previewStarted;
    staleSend();
    await tick();
    assert.deepEqual(f.canceled, []);
    assert.equal(f.sendCalls(), 0);
    f.closePreview();
    await tick();
    assert.equal(f.sendCalls(), 0);
    f.render().field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 1);
  },
);

test(
  'an explicit Send still drains a legitimately queued caption save before sealing the draft',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const gate = deferred<void>();
    f.holdSaves(gate.promise);
    const ui = f.render();
    ui.field!.onChange('Queued caption 🪁');
    ui.field!.onSend();
    await tick();
    assert.equal(f.sendCalls(), 0);
    gate.resolve();
    await f.channel.flush();
    assert.equal(f.sendCalls(), 1);
    assert.equal(JSON.parse(f.sent[0]!).items[0].caption, 'Queued caption 🪁');
  },
);
