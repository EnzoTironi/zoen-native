import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nativeEditorFixture } from './article-native-editor.fixtures.ts';

test('installed ENRM refreshes its public imperative handle for the same mounted Fabric editor', () => {
  const f = nativeEditorFixture();
  f.render();
  const first = f.nativeHandle();
  assert.ok(first);
  f.render();
  assert.notEqual(f.nativeHandle(), first);
  assert.equal(typeof first.getMarkdown, 'function');
  assert.equal(f.calls.length, 0);
});

test('actual Article Link stays current across installed ENRM handle refresh while Markdown capture awaits', async () => {
  const f = nativeEditorFixture();
  f.render();
  f.select(7, 15);
  f.render();
  const link = f.find('attachmentTabs.link');
  assert.ok(link);
  assert.equal(link.props.disabled, false);
  link.props.onPress();
  f.render(); // linkBusy updates editable, so the actual ENRM component creates its next handle.
  assert.equal(f.leaf().props.editable, false);
  await f.reply();
  assert.ok(
    f.find('attachmentTabs.linkUrl'),
    'same mounted native editor must open the selected-text URL form',
  );
});

test('actual ENRM URL apply survives further facade refresh and formats only the original selection once', async () => {
  const f = nativeEditorFixture();
  f.render();
  f.select(7, 15);
  f.render();
  f.find('attachmentTabs.link')!.props.onPress();
  f.render();
  await f.reply();
  f.find('attachmentTabs.linkUrl')!.props.onChangeText('https://example.invalid/selected');
  f.render();
  const apply = f.find('attachmentTabs.applyLink')!;
  assert.equal(apply.props.disabled, false);
  apply.props.onPress();
  apply.props.onPress();
  f.render();
  await f.reply();
  f.render();
  const effects = f.calls.filter((call) => call[0] === 'setSelection' || call[0] === 'setLink');
  assert.deepEqual(
    effects.map((call) => [call[0], ...call.slice(2)]),
    [
      ['setSelection', 7, 15],
      ['setLink', 'https://example.invalid/selected'],
    ],
  );
  assert.equal(f.calls.filter((call) => call[0] === 'changed').length, 0);
  assert.equal(f.find('attachmentTabs.linkUrl'), undefined);
});

test('actual ENRM selection change during capture still rejects stale selected offsets', async () => {
  const f = nativeEditorFixture();
  f.render();
  f.select(7, 15);
  f.render();
  f.find('attachmentTabs.link')!.props.onPress();
  f.render();
  f.select(0, 0);
  f.render();
  await f.reply();
  assert.equal(f.find('attachmentTabs.linkUrl'), undefined);
  assert.equal(
    f.calls.some((call) => call[0] === 'setLink'),
    false,
  );
});

test('actual ENRM byte drift and retired owner lease refuse Link despite a stable mount facade', async () => {
  for (const change of ['bytes', 'lease'] as const) {
    const f = nativeEditorFixture();
    f.render();
    f.select(7, 15);
    f.render();
    f.find('attachmentTabs.link')!.props.onPress();
    f.render();
    await f.reply();
    f.find('attachmentTabs.linkUrl')!.props.onChangeText('https://example.invalid');
    f.render();
    f.find('attachmentTabs.applyLink')!.props.onPress();
    f.render();
    if (change === 'lease') {
      f.retire();
      f.resume();
    }
    await f.reply(change === 'bytes' ? 'replacement content' : 'before selected after');
    assert.equal(
      f.calls.some((call) => call[0] === 'setSelection' || call[0] === 'setLink'),
      false,
      change,
    );
    assert.equal(f.find('attachmentTabs.linkUrl'), undefined);
  }
});

test('replacement native child retires the captured editor identity rather than transferring a Link to the new view', async () => {
  const f = nativeEditorFixture();
  f.render();
  f.select(7, 15);
  f.render();
  f.find('attachmentTabs.link')!.props.onPress();
  f.render();
  await f.reply();
  f.find('attachmentTabs.linkUrl')!.props.onChangeText('https://example.invalid');
  f.render();
  const apply = f.find('attachmentTabs.applyLink')!;
  f.replaceInput();
  apply.props.onPress();
  f.render();
  assert.equal(
    f.calls.some((call) => call[0] === 'setLink' || call[0] === 'setSelection'),
    false,
  );
  assert.equal(f.find('attachmentTabs.linkUrl'), undefined);
});
