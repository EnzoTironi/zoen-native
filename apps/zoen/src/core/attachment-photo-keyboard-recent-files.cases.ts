import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AttachmentAsset } from '../media/attachment-draft';
import { recentAttachmentId } from '../media/attachment-recent-session.ts';
import { fixture, recentFixture, recentScope, tick } from './attachment-photo-keyboard.fixtures.ts';

test('real File recent projection sends exact owned copies and final caption only after native removal', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  const first = f.render(),
    model = JSON.parse(first.props.model);
  assert.deepEqual(
    Array.from(model.fileItems, (row: any) => row.id),
    r.rows.map(recentAttachmentId),
  );
  assert.deepEqual(model.audioItems, []);
  f.act(first, 'recentSelect', recentAttachmentId(r.rows[1]!));
  f.act(f.render(), 'recentSelect', recentAttachmentId(r.rows[0]!));
  const selected = f.render(),
    selectedModel = JSON.parse(selected.props.model);
  assert.equal(selectedModel.caption.count, 2);
  assert.equal(selectedModel.caption.sendEnabled, true);
  f.act(selected, 'document');
  assert.equal(
    f.photos.getSnapshot().visible,
    true,
    'Browse cannot drop existing recent selections',
  );
  f.act(selected, 'recentSend', undefined, { text: 'Final native caption' });
  await tick();
  assert.deepEqual(r.materialized, ['b', 'a']);
  assert.equal(f.picked(), undefined);
  assert.equal(f.submission()?.send, true);
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final native caption', '']);
  f.close(selected);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((file) => file.uri),
    ['file:///native-owned/b', 'file:///native-owned/a'],
  );
  assert.equal(
    (f.picked() as AttachmentAsset[]).some((file) => 'reference' in file),
    false,
  );
  r.recent.retire();
  f.retire();
});

test('failed recent Send cannot leak its submission intent into an ordinary Browse result', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  r.fail();
  f.act(f.render(), 'recentOpen', recentAttachmentId(r.rows[0]!));
  await tick();
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.submission(), undefined);
  const browsing = f.render();
  f.act(browsing, 'document');
  assert.equal(f.submission()?.send, false);
  f.close(browsing);
  await tick();
  assert.equal((f.picked() as AttachmentAsset[])[0]?.name, 'document.pdf');
  assert.equal(f.submission()?.send, false);
  r.recent.retire();
  f.retire();
});

test('recent Preview waits for native removal and resumes the same File child without preparing Send', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  const tree = f.render();
  f.act(tree, 'recentPreview', recentAttachmentId(r.rows[0]!));
  assert.deepEqual(r.previews, []);
  assert.equal(f.photos.getSnapshot().visible, false);
  f.close(tree);
  await tick();
  assert.deepEqual(r.previews, ['a']);
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.photos.getSnapshot().category, 'document');
  assert.equal(f.submission(), undefined);
  assert.deepEqual(r.materialized, []);
  assert.equal(f.picked(), undefined);
  assert.equal(f.resumed.length, 1);
  r.recent.retire();
  f.retire();
});

test('recent state revision rejects an old row action after search activation', async () => {
  const r = recentFixture(),
    f = await fixture(true, undefined, r.recent);
  f.photos.showCategory('document');
  r.recent.activate({ scope: recentScope, port: r.port }, 'file', 3, () =>
    f.photos.isCurrent(f.photos.getSnapshot().token),
  );
  await tick();
  const old = f.render();
  f.act(old, 'recentSearch');
  f.render();
  f.act(old, 'recentOpen', recentAttachmentId(r.rows[0]!));
  await tick();
  assert.deepEqual(r.materialized, []);
  assert.equal(f.submission(), undefined);
  r.recent.retire();
  f.retire();
});

test('Audio Browse always selects ordinary audio files and never falls back to voice recording', async () => {
  const f = await fixture(true);
  f.browseNext(async () => [
    { uri: 'file:///owned/music.m4a', name: 'music.m4a', mime: 'audio/mp4', size: 12 },
  ]);
  f.act(f.render(), 'category', 'audio');
  const audio = f.render();
  f.act(audio, 'audio');
  assert.equal(f.submission()?.send, false);
  assert.equal(f.photos.getSnapshot().visible, false);
  f.close(audio);
  await tick();
  assert.deepEqual(
    f.calls.filter((call) => call === 'audio'),
    [],
  );
  assert.equal(f.calls.includes('browse'), true);
  assert.equal((f.picked() as AttachmentAsset[])[0]?.mime, 'audio/mp4');
  assert.equal('voiceSourceId' in (f.picked() as AttachmentAsset[])[0]!, false);
  f.retire();
});
