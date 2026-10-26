import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { appOrientationLayout } from './app-orientation-react.fixtures.ts';
import { appSheetCloseFixture } from '../miniapps/app-sheet-close-react.fixtures.ts';

test('compiled root keeps chat and normal navigation portrait while the fullscreen App owns both orientations', async () => {
  const root = await appOrientationLayout();
  for (const name of [
    '(tabs)',
    'chat/[id]',
    'chat-attachment/[id]',
    'miniapps',
    'new-chat',
    'account-budget',
    'item/[id]',
    'settings/appearance',
  ])
    assert.equal(root.options(name).orientation, 'portrait', name);
  assert.equal(root.options('miniapp').orientation, 'default');
  assert.equal(root.options('miniapp').presentation, 'fullScreenModal');
  const app = await appOrientationLayout(true);
  assert.equal(app.options('[id]').orientation, 'default');
  assert.equal(app.options('[id]').headerShown, false);
  assert.equal(app.options('[id]/manage').orientation, 'default');
  const manifest = JSON.parse(readFileSync(new URL('../../app.json', import.meta.url), 'utf8'));
  assert.equal(
    manifest.expo.orientation,
    'default',
    'a whole-app portrait manifest would prevent App landscape',
  );
  assert.deepEqual(root.errors, []);
  assert.deepEqual(app.errors, []);
});

test('actual App removal guard retains App orientation on failure and exposes chat portrait only after flush', async () => {
  const root = await appOrientationLayout();
  const f = await appSheetCloseFixture({ origin: 'chat' });
  try {
    assert.equal(root.options(f.current().name).orientation, 'default');
    const retainedClose = f.qa.button.onPress;
    await f.act(retainedClose);
    assert.equal(root.options(f.current().name).orientation, 'default');
    assert.deepEqual(f.flushes, [{ retire: true }]);
    await f.act(() => f.pending[0]!.reject(Error('controlled persistence failure')));
    assert.equal(f.current().name, 'miniapp');
    assert.equal(root.options(f.current().name).orientation, 'default');
    await f.act(() => f.qa.button.onPress());
    await f.act(() => f.pending[1]!.resolve());
    assert.equal(f.current().name, 'chat');
    assert.equal(root.options(f.current().name).orientation, 'portrait');
    assert.equal(f.qa.disposed, 1);
    await f.act(retainedClose);
    assert.equal(root.options(f.current().name).orientation, 'portrait');
    assert.equal(f.flushes.length, 2);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('closing App management restores the ordinary screen mask without a document write', async () => {
  const root = await appOrientationLayout();
  const f = await appSheetCloseFixture({ manage: true });
  try {
    assert.equal(root.options(f.current().name).orientation, 'default');
    await f.act(() => f.qa.button.onPress());
    assert.equal(f.current().name, 'library');
    assert.equal(root.options(f.current().name).orientation, 'portrait');
    assert.deepEqual(f.flushes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
