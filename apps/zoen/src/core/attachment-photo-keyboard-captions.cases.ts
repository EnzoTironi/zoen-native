import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
import { fixture, item, tick } from './attachment-photo-keyboard.fixtures.ts';

test('gallery Send includes its current unselected item and the final native keystroke only after explicit Send', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'preview', 'ph://b');
  const tree = f.render();
  assert.deepEqual(f.photos.getSnapshot().selected, []);
  assert.equal(JSON.parse(tree.props.model).caption.sendEnabled, true);
  assert.equal(JSON.parse(tree.props.model).caption.count, 0);
  assert.equal(JSON.parse(tree.props.model).caption.sendLabel, 'media.attachmentSendCount:1');
  assert.equal(JSON.parse(tree.props.model).labels.edit, 'media.attachmentEdit');
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.act(tree, 'send', 'ph://b', { text: 'Final gallery keystroke!' });
  f.act(tree, 'send', 'ph://b', { text: 'Duplicate' });
  await tick();
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submission()?.caption, 'Final gallery keystroke!');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final gallery keystroke!']);
  assert.deepEqual(f.copies, [['ph://b']]);
  assert.equal(f.picked(), undefined);
  f.close(tree);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((value) => value.uri),
    ['file:///owned/b'],
  );
});

test('gallery caption and Send reject missing, foreign and noncurrent item identities without changing selection', async () => {
  const f = await fixture(true);
  f.photos.select(item('a'));
  f.act(f.render(), 'preview', 'ph://b');
  const tree = f.render();
  for (const id of ['ph://foreign', 'ph://a']) {
    f.act(tree, 'caption', id, { text: 'Wrong photo' });
    f.act(tree, 'send', id, { text: 'Wrong photo' });
  }
  f.act(tree, 'caption', undefined, { text: 'Missing gallery identity' });
  f.act(tree, 'send', undefined, { text: 'Missing gallery identity' });
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  assert.equal(JSON.parse(f.render().props.model).caption.value, '');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  f.retire();
  await tick();
});

test('grid Send rejects a supplied gallery identity and an empty grid remains unsendable', async () => {
  const f = await fixture(true);
  assert.equal(JSON.parse(f.render().props.model).caption.sendEnabled, false);
  f.act(f.render(), 'send', undefined, { text: 'No selection' });
  f.photos.select(item('a'));
  f.act(f.render(), 'send', 'ph://foreign', { text: 'Unexpected gallery identity' });
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.retire();
  await tick();
});

test('per-photo gallery captions survive navigation and follow the exact reordered selection on Send', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'preview', 'ph://a');
  f.act(f.render(), 'caption', 'ph://a', { text: 'Caption A' });
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  f.act(f.render(), 'preview', 'ph://b');
  assert.equal(JSON.parse(f.render().props.model).caption.value, '');
  f.act(f.render(), 'caption', 'ph://b', { text: 'Caption B before final key' });
  f.act(f.render(), 'preview', 'ph://a');
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'Caption A');
  f.act(f.render(), 'reorder', undefined, { ids: ['ph://b', 'ph://a'] });
  f.act(f.render(), 'preview', 'ph://b');
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'Caption B before final key');
  const tree = f.render();
  f.act(tree, 'send', 'ph://b', { text: 'Caption B before final key!' });
  await tick();
  assert.equal(f.submission()?.caption, 'Caption B before final key!');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), [
    'Caption B before final key!',
    'Caption A',
  ]);
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  f.close(tree);
  await tick();
});

test('a grid common caption replaces item captions on the first selected asset without leaking across Photos owners', async () => {
  const f = await fixture(true);
  f.act(f.render(), 'preview', 'ph://a');
  f.act(f.render(), 'caption', 'ph://a', { text: 'Old per-photo caption' });
  f.act(f.render(), 'back');
  f.photos.select(item('b'));
  f.act(f.render(), 'caption', undefined, { text: 'Common caption' });
  f.act(f.render(), 'preview', 'ph://b');
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'Common caption');
  const tree = f.render();
  f.act(tree, 'send', 'ph://b', { text: 'Common caption final' });
  await tick();
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Common caption final', '']);
  assert.deepEqual(f.copies, [['ph://a', 'ph://b']]);
  f.close(tree);
  await tick();
  const next = await fixture(true);
  next.act(next.render(), 'preview', 'ph://a');
  assert.equal(JSON.parse(next.render().props.model).caption.value, '');
  next.retire();
  await tick();
});

test('supported Photos video Edit stages the current exact selection for the durable owner without sending', async () => {
  for (const extension of ['mp4', 'mov', 'm4v']) {
    const video = {
      ...item('video'),
      filename: `clip.${extension}`,
      mediaType: 'video' as PhotoLibraryItem['mediaType'],
    };
    const f = await fixture(true, [item('a'), video]);
    f.photos.select(item('a'));
    f.act(f.render(), 'preview', video.id);
    const tree = f.render();
    assert.equal(JSON.parse(tree.props.model).canEdit, true);
    f.act(tree, 'edit', video.id, { text: 'Final video caption' });
    f.act(tree, 'edit', video.id, { text: 'Duplicate' });
    await tick();
    assert.equal(f.submissions.length, 1);
    assert.equal(f.submission()?.send, false);
    assert.equal(f.submission()?.requestEdit, true);
    assert.equal(f.submission()?.caption, '');
    assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['', 'Final video caption']);
    assert.deepEqual(f.copies, [['ph://a', video.id]]);
    assert.equal(f.preferred(), 1);
    assert.equal(f.picked(), undefined, 'Editing must wait for the native presentation to close');
    f.close(tree);
    await tick();
    assert.equal((f.picked() as AttachmentAsset[])[1]?.name, `clip.${extension}`);
    f.retire();
  }
});

test('unsupported Photos video format cannot prepare an edit or fall through to Send', async () => {
  const video = {
    ...item('video'),
    filename: 'clip.webm',
    mediaType: 'video' as PhotoLibraryItem['mediaType'],
  };
  const f = await fixture(true, [video]);
  f.act(f.render(), 'preview', video.id);
  const tree = f.render();
  assert.equal(JSON.parse(tree.props.model).canEdit, false);
  f.act(tree, 'edit', video.id, { text: 'Unsupported' });
  await tick();
  assert.deepEqual(f.submissions, []);
  assert.deepEqual(f.copies, []);
  f.retire();
});

test('a caption-only journal projection does not retire the still-observed native input revision', async () => {
  const f = await fixture(true);
  f.photos.select(item('a'));
  f.act(f.render(), 'preview', 'ph://a');
  const observed = f.render();
  f.act(observed, 'caption', 'ph://a', { text: 'A' });
  const updated = f.render();
  assert.equal(JSON.parse(updated.props.model).revision, JSON.parse(observed.props.model).revision);
  f.act(observed, 'caption', 'ph://a', { text: 'AB' });
  assert.equal(JSON.parse(f.render().props.model).caption.value, 'AB');
  f.act(f.render(), 'preview', 'ph://b');
  f.act(observed, 'caption', 'ph://a', { text: 'Foreign late keystroke' });
  assert.equal(f.photos.getSnapshot().captions.items['ph://a'], 'AB');
  f.retire();
  await tick();
});

test('native grid and gallery use the captured copy and metadata instead of the mutable Photos row', async () => {
  const f = await fixture(true);
  f.photos.select(item('a'));
  const original = f.photos.getSnapshot();
  const state = {
    ...original,
    items: [{ ...item('a'), filename: 'replacement.jpg', width: 900 }],
    previewUris: { 'ph://a': null },
  };
  const pending = JSON.parse(f.render(state).props.model);
  assert.equal(pending.items[0].retained, true);
  assert.equal(pending.items[0].uri, null, 'pending copy must not fall back to PHAsset');
  assert.equal(pending.items[0].name, 'a.jpg');
  const ready = JSON.parse(
    f.render({ ...state, previewUris: { 'ph://a': 'file:///cache/exact-captured.jpg' } }).props
      .model,
  );
  assert.equal(ready.items[0].uri, 'file:///cache/exact-captured.jpg');
  assert.equal(ready.items[0].width, 100);
  assert.deepEqual(ready.items[0], ready.selectedItems[0]);
  assert.equal(
    ready.revision,
    pending.revision,
    'thumbnail IO does not revoke the active caption input',
  );
  f.retire();
  await tick();
});

for (const mode of ['send', 'send-as-file', 'send-without-grouping'] as const) {
  test(`Back from mixed review preserves each caption and reordered asset for grid ${mode}`, async () => {
    const video = {
      ...item('b'),
      filename: 'original.mov',
      mediaType: 'video' as PhotoLibraryItem['mediaType'],
    };
    const f = await fixture(true, [item('a'), video], undefined, true);
    f.act(f.render(), 'preview', 'ph://a');
    f.act(f.render(), 'caption', 'ph://a', { text: 'Photo 🖼️\noriginal caption' });
    f.act(f.render(), 'preview', video.id);
    f.act(f.render(), 'caption', video.id, { text: 'Video caption' });
    f.act(f.render(), 'reorder', undefined, { ids: [video.id, 'ph://a'] });
    const review = f.render();
    f.act(review, 'back');
    const grid = f.render();
    assert.equal(JSON.parse(grid.props.model).caption.value, '');
    assert.deepEqual(f.copies, [], 'Back is navigation, not an export or Send');
    f.act(review, 'send', video.id, { text: 'Retired review' });
    assert.deepEqual(f.submissions, []);
    f.act(grid, mode === 'send' ? 'send' : 'option', mode === 'send' ? undefined : mode, {
      text: '',
      ids: [video.id, 'ph://a'],
    });
    await tick();
    assert.equal(f.submissions.length, 1);
    assert.equal(f.submission()?.send, true);
    assert.deepEqual(Array.from(f.submission()?.captions ?? []), [
      'Video caption',
      'Photo 🖼️\noriginal caption',
    ]);
    assert.deepEqual(f.copies, [[video.id, 'ph://a']]);
    assert.equal(f.picked(), undefined, 'even explicit Send waits for actual native removal');
    f.close(grid);
    await tick();
    assert.deepEqual(
      (f.picked() as AttachmentAsset[]).map(({ uri, name, mime }) => ({ uri, name, mime })),
      [
        { uri: 'file:///owned/b', name: 'original.mov', mime: 'video/quicktime' },
        { uri: 'file:///owned/a', name: 'a.jpg', mime: 'image/jpeg' },
      ],
    );
  });
}

for (const common of ['Final common caption', '']) {
  test(`an explicit grid caption ${common ? 'typed at Send' : 'cleared before Send'} replaces per-item review captions`, async () => {
    const f = await fixture(true);
    f.act(f.render(), 'preview', 'ph://a');
    f.act(f.render(), 'caption', 'ph://a', { text: 'Keep unless explicitly replaced' });
    f.act(f.render(), 'preview', 'ph://b');
    f.act(f.render(), 'caption', 'ph://b', { text: 'Second caption' });
    f.act(f.render(), 'back');
    if (common === '') f.act(f.render(), 'caption', undefined, { text: '' });
    const grid = f.render();
    f.act(grid, 'send', undefined, { text: common });
    await tick();
    assert.deepEqual(Array.from(f.submission()?.captions ?? []), [common, '']);
    f.close(grid);
    await tick();
  });
}
