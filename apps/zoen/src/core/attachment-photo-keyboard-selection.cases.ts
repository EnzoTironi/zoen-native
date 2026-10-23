import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft';
import { deferred, fixture, item, tick } from './attachment-photo-keyboard.fixtures.ts';

test('actual native projection carries ordered selection/permission/palette and exact field binding without opening a sheet', async () => {
  const f = await fixture();
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render(),
    binding = JSON.parse(tree.props.binding),
    model = JSON.parse(tree.props.model);
  assert.equal(tree.type, 'ZoenKeyboardPanelHost');
  assert.equal(binding.tag, 7);
  assert.equal(binding.context, 'space:normal');
  assert.equal(binding.preferredHeight, 336);
  assert.equal(binding.active, true);
  assert.equal(binding.appearance.background, '#000000');
  assert.deepEqual(model.selected, ['ph://b', 'ph://a']);
  assert.equal(model.columns, 3);
  assert.equal(model.hasAudio, true);
  f.retire();
  await tick();
});

test('caption controls are projected only for the durable submission adapter', async () => {
  const ordinary = await fixture(),
    durable = await fixture(true);
  assert.equal('caption' in JSON.parse(ordinary.render().props.model), false);
  durable.photos.select(item('a'));
  const tree = durable.render();
  assert.equal(JSON.parse(tree.props.model).caption.count, 1);
  durable.act(tree, 'caption', undefined, { text: '  Caption 🖼️\nkept exactly  ' });
  assert.equal(
    JSON.parse(durable.render().props.model).caption.value,
    '  Caption 🖼️\nkept exactly  ',
  );
  assert.deepEqual(durable.submissions, []);
  ordinary.retire();
  durable.retire();
  await tick();
});

test('direct Send takes the final native text and awaits native removal before releasing selected files', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  let tree = f.render();
  f.act(tree, 'caption', undefined, { text: 'Before the final keystroke' });
  tree = f.render();
  const finalCaption = 'Before the final keystroke!';
  f.act(tree, 'send', undefined, { text: finalCaption });
  // The mounted callback still has the old model text. A second tap cannot submit again.
  f.act(tree, 'send', undefined, { text: 'duplicate' });
  await tick();
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submissions[0]!.caption, finalCaption);
  assert.equal(f.submissions[0]!.send, true);
  assert.equal(f.preferred(), null);
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.picked(), undefined, 'preparing bytes must not complete the still-mounted sheet');
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((value) => value.uri),
    ['file:///owned/b', 'file:///owned/a'],
  );
});

test('caption and Send reject malformed text and foreign, stale or retired presentation callbacks', async () => {
  const f = await fixture(true);
  f.photos.select(item('a'));
  const stale = f.render();
  f.photos.select(item('b'));
  const tree = f.render();
  for (const text of ['x'.repeat(4097), 'nul\0text', 42]) {
    f.act(tree, 'caption', undefined, { text });
    f.act(tree, 'send', undefined, { text });
  }
  for (const payload of [
    { token: 'other' },
    { context: 'other-chat' },
    { revision: 'other-render' },
  ]) {
    f.act(tree, 'caption', undefined, { text: 'foreign', ...payload });
    f.act(tree, 'send', undefined, { text: 'foreign', ...payload });
  }
  f.act(stale, 'send', undefined, { text: 'old selection' });
  assert.equal(JSON.parse(f.render().props.model).caption.value, '');
  f.retire();
  f.act(tree, 'send', undefined, { text: 'retired' });
  await tick();
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
});

test('direct Send admits a 4096-character caption but rejects an empty or busy selection', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'send', undefined, { text: 'nothing selected' });
  assert.deepEqual(f.submissions, []);
  f.photos.select(item('a'));
  f.act(f.render({ ...f.photos.getSnapshot(), busy: true }), 'send', undefined, { text: 'busy' });
  assert.deepEqual(f.submissions, []);
  const tree = f.render();
  f.act(tree, 'send', undefined, { text: 'x'.repeat(4096) });
  await tick();
  assert.equal(f.submissions.length, 1);
  const submission = f.submission();
  assert.ok(submission);
  assert.equal(submission.caption.length, 4096);
  f.close(tree);
  await tick();
});

test('Photos More sends only an admitted file intent with exact ordered IDs and final native caption after export', async () => {
  const f = await fixture(true, undefined, undefined, true);
  assert.deepEqual(JSON.parse(f.render().props.model).sendOptions, []);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render(),
    menu = JSON.parse(tree.props.model).sendOptions;
  assert.equal(menu.length, 2);
  assert.equal(menu[0].id, 'send-as-file');
  assert.equal(menu[0].enabled, true);
  const exportReady = deferred<AttachmentAsset[]>();
  f.materializeNext(() => exportReady.promise);
  f.act(tree, 'option', 'send-as-file', {
    ids: ['ph://b', 'ph://a'],
    text: 'Final file caption 🪁',
  });
  assert.deepEqual(f.submissions, [], 'No terminal mode before materialization succeeds');
  exportReady.resolve([
    { uri: 'file:///owned/b', mime: 'image/jpeg', name: 'b.jpg', size: 12 },
    { uri: 'file:///owned/a', mime: 'image/jpeg', name: 'a.jpg', size: 12 },
  ]);
  await tick();
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.submission()?.caption, 'Final file caption 🪁');
  assert.equal(f.submission()?.options?.presentation, 'file');
  assert.equal(f.submission()?.options?.grouping, 'separate');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final file caption 🪁', '']);
  assert.equal(f.picked(), undefined, 'The actual close barrier still owns handoff');
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((file) => file.name),
    ['b.jpg', 'a.jpg'],
  );
});

test('file option rejects unsupported mode, stale selection/revision, malformed caption and disabled admission', async () => {
  const blocked = await fixture(true);
  blocked.photos.select(item('a'));
  const unavailable = blocked.render();
  assert.deepEqual(JSON.parse(unavailable.props.model).sendOptions, []);
  blocked.act(unavailable, 'option', 'send-as-file', { ids: ['ph://a'], text: 'blocked' });
  assert.deepEqual(blocked.copies, []);
  blocked.retire();
  const f = await fixture(true, undefined, undefined, true);
  f.photos.select(item('a'));
  const old = f.render();
  f.photos.select(item('b'));
  const tree = f.render();
  for (const [id, payload] of [
    ['unknown', { ids: ['ph://a', 'ph://b'], text: 'bad option' }],
    ['send-as-file', { ids: ['ph://b', 'ph://a'], text: 'wrong order' }],
    ['send-as-file', { ids: ['ph://a'], text: 'missing member' }],
    ['send-as-file', { ids: ['ph://a', 'ph://b'], text: 'bad\0caption' }],
    ['send-as-file', { ids: ['ph://a', 'ph://b'], text: 'x'.repeat(4097) }],
    ['send-as-file', { ids: ['ph://a', 'ph://b'], text: 'foreign', revision: 'foreign' }],
  ] as const)
    f.act(tree, 'option', id, payload);
  f.act(old, 'option', 'send-as-file', { ids: ['ph://a'], text: 'stale menu' });
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.retire();
  f.act(tree, 'option', 'send-as-file', { ids: ['ph://a', 'ph://b'], text: 'retired' });
  assert.deepEqual(f.copies, []);
});

test('failed as-file export leaves no mode for a later ordinary media send', async () => {
  const f = await fixture(true, undefined, undefined, true);
  f.photos.select(item('a'));
  f.materializeNext(async () => {
    throw new Error('export failed');
  });
  f.act(f.render(), 'option', 'send-as-file', { ids: ['ph://a'], text: 'failed file intent' });
  await tick();
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.deepEqual(f.submissions, []);
  f.materializeNext(async () => [
    { uri: 'file:///owned/a', mime: 'image/jpeg', name: 'a.jpg', size: 12 },
  ]);
  const ordinary = f.render();
  f.act(ordinary, 'send', undefined, { text: 'ordinary caption' });
  await tick();
  assert.equal(f.submission()?.options, undefined);
  assert.equal(f.submission()?.caption, 'ordinary caption');
  f.close(ordinary);
  await tick();
});

test('retirement during as-file export cannot record a terminal file submission', async () => {
  const f = await fixture(true, undefined, undefined, true),
    pending = deferred<AttachmentAsset[]>();
  f.photos.select(item('a'));
  f.materializeNext(() => pending.promise);
  f.act(f.render(), 'option', 'send-as-file', { ids: ['ph://a'], text: 'canceled' });
  f.retire();
  pending.resolve([{ uri: 'file:///owned/a', mime: 'image/jpeg', name: 'a.jpg', size: 12 }]);
  await tick();
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.picked(), []);
});

test('removed legacy Review cannot prepare files and native Preview stays inside the same Photos task', async () => {
  const review = await fixture(true);
  review.photos.select(item('a'));
  review.act(review.render(), 'caption', undefined, { text: 'Review this first' });
  const tree = review.render();
  review.act(tree, 'review');
  await tick();
  assert.deepEqual(review.submissions, []);
  assert.deepEqual(review.copies, []);
  assert.equal(review.photos.getSnapshot().visible, true);
  assert.equal(review.picked(), undefined);
  review.retire();
  await tick();

  const preview = await fixture(true);
  preview.photos.select(item('a'));
  const token = preview.photos.getSnapshot().token;
  preview.act(preview.render(), 'preview', 'ph://b');
  await tick();
  assert.equal(preview.photos.getSnapshot().screen, 'preview');
  assert.equal(preview.photos.getSnapshot().previewId, 'ph://b');
  assert.equal(preview.photos.getSnapshot().visible, true);
  assert.equal(preview.photos.getSnapshot().token, token);
  assert.deepEqual(
    preview.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  assert.deepEqual(preview.submissions, []);
  assert.deepEqual(preview.copies, []);
  assert.equal(preview.picked(), undefined);
  preview.act(preview.render(), 'back');
  assert.equal(preview.photos.getSnapshot().screen, 'grid');
  preview.retire();
  await tick();
});
