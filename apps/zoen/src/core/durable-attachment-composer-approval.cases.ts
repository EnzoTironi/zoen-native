import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft.ts';
import {
  deferred,
  empty,
  findStrip,
  fixture,
  photo,
  scope,
  tick,
} from './durable-attachment-composer.fixtures.ts';

test('full-screen approval opens after media selection, preserves each caption and captured thread target, and Close keeps the draft', async () => {
  const f = fixture(empty(), { ...scope, thread_root_id: 'thread-root' });
  f.render();
  await tick();
  f.select([photo('one.jpg'), { ...photo('two.mov'), mime: 'video/quicktime' }]);
  await f.render({ replyTo: 'captured-reply' }).pick('photo');
  let approval = f.approval()!;
  assert.ok(approval);
  assert.equal(approval.recipient, 'Synthetic conversation');
  assert.equal(approval.field.scope.replyTo, 'captured-reply');
  assert.equal(approval.field.scope.thread, true);
  assert.equal(f.previews.length, 0, 'Approval does not automatically open the native editor');
  approval.field.onChange('Photo caption');
  await f.channel.flush();
  f.render({ replyTo: 'later-reply' });
  approval = f.approval()!;
  approval.onSelect('asset-2');
  approval = f.approval()!;
  approval.field.onChange('Video caption');
  await f.channel.flush();
  assert.equal(approval.field.scope.replyTo, 'captured-reply');
  const staleSend = approval.field.onSend;
  approval.onClose();
  await tick();
  assert.equal(f.approval(), undefined);
  assert.deepEqual(
    JSON.parse(f.durable().content_json).items.map((item: { caption: string }) => item.caption),
    ['Photo caption', 'Video caption'],
  );
  staleSend();
  await tick();
  assert.equal(
    f.sendCalls(),
    0,
    'A dismissed presentation cannot send through its retained callback',
  );
  f.openNativePreview('asset-2');
  await f.previewStarted;
  f.closePreview();
  await tick();
  f.approval()!.field.onSend();
  await tick();
  assert.equal(f.sendCalls(), 1);
  assert.equal(JSON.parse(f.sent[0]!).reply_to, 'captured-reply');
});

test('recovered attachments remain compact until explicitly expanded and expanding never stages new content', async () => {
  const f = fixture({
    revision: 'saved',
    content_json: JSON.stringify({
      kind: 'attachments',
      reply_to: null,
      items: [{ asset_id: 'old', caption: 'Recovered', mutation_id: 'old-post' }],
    }),
    assets: [{ id: 'old', name: 'old.jpg', mime: 'image/jpeg', size: 12, sha256: 'a'.repeat(64) }],
    attempted: false,
  });
  f.render();
  await tick();
  f.render();
  await tick();
  assert.equal(f.approval(), undefined);
  const strip = findStrip(f.render().review);
  (strip.props.onPreview as (id: string) => void)('old-post');
  const approval = f.approval()!;
  assert.equal(approval.field.value, 'Recovered');
  assert.equal(approval.state.items[0]!.uri, 'file:///cache/fresh-old');
  assert.equal(f.writes.length, 0);
  assert.equal(f.sendCalls(), 0);
  approval.onClose();
  await tick();
  assert.equal(f.approval(), undefined);
  f.unmount();
  await f.channel.flush();
  assert.deepEqual(f.released, ['file:///cache/fresh-old']);
});

test('adding to a recovered PDF preserves its mounted editor admission while freezing canonical caption and Send', async () => {
  const initial = {
    revision: 'saved',
    content_json: JSON.stringify({
      kind: 'attachments',
      reply_to: null,
      items: [{ asset_id: 'old', mutation_id: 'old-post', caption: 'Keep this PDF caption' }],
    }),
    attempted: false,
    assets: [
      {
        id: 'old',
        name: 'QA230 Input-v7.pdf',
        mime: 'application/pdf',
        size: 12,
        sha256: 'a'.repeat(64),
      },
    ],
  };
  const f = fixture(initial),
    removed = deferred<AttachmentAsset[]>();
  f.render();
  await tick();
  let ui = f.render();
  await tick();
  ui = f.render();
  assert.equal(ui.canAdd, true);
  assert.equal(ui.field?.editable, true);
  const retained = ui.field!;
  f.select(() => removed.promise);
  const adding = ui.pick('photo');
  await tick();
  ui = f.render();
  assert.equal(ui.busy, true);
  assert.equal(
    ui.field?.editable,
    true,
    'Native mounted attachment admission requires the same underlying UITextView to remain editable',
  );
  retained.onChange('Late underlying caption');
  ui.field!.onChange('Typing behind the attachment');
  retained.onSend();
  ui.field!.onSend();
  await tick();
  assert.equal(f.writes.length, 0, 'Adding cannot mutate the old canonical caption');
  assert.equal(f.sendCalls(), 0);
  removed.resolve([]);
  await adding;
  assert.equal(f.durable().content_json, initial.content_json);
  assert.equal(f.render().field?.editable, true);
  assert.equal(f.render().canAdd, true);
  f.render().field!.onChange('User caption after closing picker');
  await f.channel.flush();
  assert.equal(
    JSON.parse(f.durable().content_json).items[0].caption,
    'User caption after closing picker',
  );
  assert.equal(f.durable().assets[0]?.name, 'QA230 Input-v7.pdf');
  assert.equal(f.sendCalls(), 0);
  f.unmount();
});

test(
  'Edit journals its final native caption before editor admission and coalesces a competing Send',
  { timeout: 10_000 },
  async () => {
    const f = fixture(),
      saved = deferred<void>();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const approval = f.approval()!;
    const finalCaption = 'Final native edit keystroke 🪁';
    f.holdSaves(saved.promise);
    approval.onEdit(finalCaption);
    approval.field.onSend('Competing Send');
    approval.onEdit('Duplicate Edit');
    await tick();
    assert.equal(f.previews.length, 0);
    assert.equal(f.sendCalls(), 0);
    saved.resolve();
    await f.previewStarted;
    assert.equal(f.previews.length, 1);
    assert.equal(JSON.parse(f.durable().content_json).items[0].caption, finalCaption);
    assert.equal(f.sendCalls(), 0);
    f.closePreview();
    await tick();
    f.unmount();
  },
);

test('a failed caption save or retired approval cannot admit an editor after a pending final caption', async () => {
  for (const kind of ['save', 'close', 'destination', 'retire'] as const) {
    const f = fixture(),
      saved = deferred<void>();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const approval = f.approval()!;
    f.holdSaves(saved.promise);
    approval.onEdit('Preserve the caption, revalidate editor admission');
    await tick();
    if (kind === 'save') f.saveFailure(true);
    else if (kind === 'close') approval.onClose();
    else if (kind === 'destination') f.render({ spaceId: 'another-chat' });
    else f.retire();
    saved.resolve();
    await f.channel.flush().catch(() => {});
    await tick();
    assert.equal(f.previews.length, 0, kind);
    assert.equal(f.sendCalls(), 0, kind);
    f.unmount();
  }
});

test('closing approval drains the pending native child, preserves an already accepted edit, and fences its old Send', async () => {
  const f = fixture();
  f.enableNativeEditing();
  f.render();
  await tick();
  f.select([photo()]);
  await f.render().pick('photo');
  let approval = f.approval()!;
  const staleSend = approval.field.onSend;
  approval.onEdit();
  await f.previewStarted;
  approval.onClose();
  await tick();
  approval = f.approval()!;
  assert.ok(approval, 'Parent presentation stays mounted until its native child drains');
  assert.equal(approval.busy, true);
  assert.deepEqual(f.canceled, ['presentation-1']);
  staleSend();
  f.saveEditedCopy();
  await tick();
  f.render();
  await tick();
  assert.equal(f.approval(), undefined);
  assert.equal(JSON.parse(f.durable().content_json).items[0].asset_id, 'edited-asset-1');
  staleSend();
  await tick();
  assert.equal(f.sendCalls(), 0);
});

test('background, revocation and destination replacement revoke approval callbacks while retaining durable content', async () => {
  for (const retire of ['background', 'retire', 'destination'] as const) {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const old = f.approval()!;
    assert.ok(old);
    if (retire === 'destination') f.render({ spaceId: 'another-chat' });
    else f[retire]();
    await tick();
    assert.equal(f.approval(), undefined);
    old.field.onSend();
    old.onEdit();
    old.onAdd('photo');
    await tick();
    assert.equal(f.sendCalls(), 0);
    assert.equal(f.previews.length, 0);
    assert.equal(f.writes.length, 1);
    assert.equal(JSON.parse(f.durable().content_json).items.length, 1);
    f.unmount();
    await f.channel.retire(false);
  }
});

test('approval Close and session retirement pause native preview playback before React can unmount it', async () => {
  for (const close of ['collapse', 'session', 'background'] as const) {
    const f = fixture();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const approval = f.approval()!;
    const playback = f.playPreview();
    assert.equal(playback.isPlaying(), true);
    assert.equal(approval.isCurrent(), true);
    if (close === 'collapse') approval.onClose();
    else if (close === 'background') f.background();
    else await f.endSession();
    assert.equal(
      playback.isPlaying(),
      false,
      'Native playback must be fenced before any new React commit',
    );
    assert.equal(
      approval.isCurrent(),
      false,
      'Late native playback events cannot reclaim the retired approval',
    );
    await tick();
    assert.equal(f.approval(), undefined);
    assert.equal(f.sendCalls(), 0);
    playback.dispose();
    f.unmount();
    await f.channel.retire(false);
  }
});

test(
  'native edited-copy review freezes input without claiming an attempted Send or retry',
  { timeout: 2000 },
  async () => {
    const f = fixture();
    f.enableNativeEditing();
    f.render();
    await tick();
    f.select([photo()]);
    await f.render().pick('photo');
    const approval = f.approval()!;
    assert.equal(approval.field.retry, false);
    approval.onEdit();
    await f.previewStarted;
    try {
      const reviewing = f.approval()!;
      assert.equal(f.channel.getSnapshot().reviewing, true);
      assert.equal(
        f.channel.getSnapshot().frozen,
        true,
        'Native review must retain the writer barrier',
      );
      assert.equal(reviewing.editing, true);
      assert.equal(reviewing.field.retry, false, 'Editing is not an ambiguous Send');
      assert.equal(
        reviewing.state.items[0]!.attempted,
        false,
        'The strip must not claim an attempted Send',
      );
      assert.equal(reviewing.field.editable, false);
      assert.equal(reviewing.field.sending, true);
      assert.equal(f.sendCalls(), 0);
    } finally {
      f.saveEditedCopy();
      await tick();
      f.render();
      await tick();
      f.unmount();
      await f.channel.retire(false);
    }
  },
);

test(
  'normal accepted native edit keeps the existing approval while privacy retirement never reopens it',
  { timeout: 4000 },
  async () => {
    for (const retirement of [null, 'close', 'background', 'retire', 'destination'] as const) {
      const f = fixture();
      f.enableNativeEditing();
      f.render();
      await tick();
      f.select([photo()]);
      await f.render({ replyTo: 'captured-reply' }).pick('photo');
      let approval = f.approval()!;
      approval.field.onChange('Keep this caption');
      await f.channel.flush();
      approval = f.approval()!;
      const staleSend = approval.field.onSend;
      approval.onEdit();
      await f.previewStarted;
      if (retirement === 'close') approval.onClose();
      else if (retirement === 'destination') f.render({ spaceId: 'other-chat' });
      else if (retirement) f[retirement]();
      await tick();
      f.saveEditedCopy();
      await tick();
      f.render();
      await tick();
      approval = f.approval()!;
      if (retirement)
        assert.equal(approval, undefined, 'An accepted edit cannot restore retired UI');
      else {
        assert.ok(approval, 'Ordinary editor completion retains the original approval');
        assert.equal(approval.field.value, 'Keep this caption');
        assert.equal(approval.field.scope.replyTo, 'captured-reply');
        assert.equal(approval.selectedId, 'post-edited-asset-1');
        assert.equal(approval.state.items[0]!.uri, 'file:///cache/fresh-edited-asset-1');
        assert.equal(approval.field.editable, true);
        assert.equal(approval.field.retry, false);
        assert.equal(approval.busy, false);
      }
      const saved = JSON.parse(f.durable().content_json);
      assert.equal(saved.items[0].caption, 'Keep this caption');
      assert.equal(saved.reply_to, 'captured-reply');
      staleSend();
      await tick();
      assert.equal(
        f.sendCalls(),
        0,
        'Old Send cannot target replacement bytes or retired approval',
      );
      f.unmount();
      await f.channel.retire(false);
    }
  },
);
