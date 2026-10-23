import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, item, tick } from './attachment-photo-keyboard.fixtures.ts';

test('default Photos Send exports a signed-group intent in selection order with the final caption', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render();
  assert.equal(JSON.parse(tree.props.model).grouped, true);
  f.act(tree, 'send', undefined, { text: 'Final album caption' });
  await tick();
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.equal(f.submission()?.options?.grouping, 'grouped');
  assert.equal(f.submission()?.options?.presentation, 'media');
  assert.deepEqual(Array.from(f.submission()?.captions ?? []), ['Final album caption', '']);
  f.close(tree);
  await tick();
  f.retire();
});

test('Send Without Grouping is a typed terminal action with exact selection, revision and close barrier', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.photos.select(item('a'));
  const tree = f.render(),
    model = JSON.parse(tree.props.model);
  assert.deepEqual(
    Array.from(model.sendOptions, (option: any) => option.id),
    ['send-without-grouping'],
  );
  f.act(tree, 'option', 'send-without-grouping', {
    ids: ['ph://a', 'ph://b'],
    text: 'wrong order',
  });
  f.act(tree, 'option', 'send-without-grouping', {
    ids: ['ph://b', 'ph://a'],
    text: 'stale',
    revision: 'stale',
  });
  assert.deepEqual(f.copies, []);
  f.act(tree, 'option', 'send-without-grouping', {
    ids: ['ph://b', 'ph://a'],
    text: 'Final separate caption',
  });
  await tick();
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submission()?.options?.grouping, 'separate');
  assert.equal(f.submission()?.options?.presentation, 'media');
  assert.equal(f.submission()?.caption, 'Final separate caption');
  assert.equal(f.picked(), undefined);
  f.close(tree);
  await tick();
  f.retire();
});

test('an existing grouped draft keeps its grouped preview and cannot be partially converted by a picker menu', async () => {
  const f = await fixture(true, undefined, undefined, false, {
    presentation: 'media',
    grouping: 'grouped',
  });
  f.photos.select(item('a'));
  f.photos.select(item('b'));
  const tree = f.render(),
    model = JSON.parse(tree.props.model);
  assert.equal(model.grouped, true);
  assert.deepEqual(model.sendOptions, []);
  f.act(tree, 'option', 'send-without-grouping', { ids: ['ph://a', 'ph://b'], text: 'convert' });
  assert.deepEqual(f.submissions, []);
  f.act(tree, 'send', undefined, { text: 'Keep grouped' });
  await tick();
  assert.equal(f.submission()?.options?.grouping, 'grouped');
  f.close(tree);
  await tick();
  f.retire();
});

test('actual mounted category projection hands Article off only after native removal; foreign category and stale model cannot navigate', async () => {
  const f = await fixture();
  const opened: string[] = [];
  const routes = {
    categories: ['article'] as const,
    open: (category: string, current: () => boolean) => {
      assert.equal(current(), true);
      opened.push(category);
    },
  };
  const initial = f.render(undefined, routes);
  assert.ok(
    JSON.parse(initial.props.model).categories.some(
      (category: { id: string }) => category.id === 'article',
    ),
  );
  f.act(initial, 'category', 'location');
  assert.equal(f.photos.getSnapshot().visible, true);
  f.act(initial, 'category', 'article');
  assert.deepEqual(opened, []);
  assert.equal(f.photos.getSnapshot().visible, false);
  const closing = f.render(undefined, routes);
  f.act(initial, 'category', 'article');
  assert.deepEqual(opened, []);
  f.close(closing);
  await tick();
  assert.deepEqual(opened, ['article']);
  assert.deepEqual(f.picked(), []);
  assert.equal(f.copies.length, 0);
  assert.equal(f.submissions.length, 0);
  f.close(closing);
  await tick();
  assert.deepEqual(opened, ['article']);
});
