import assert from 'node:assert/strict';
import test from 'node:test';
import { binaryInstallFixture } from './binary-install.fixtures.ts';

for (const editor of ['wordcraft', 'deckcraft', 'gridcraft'] as const) {
  test(`actual ${editor} catalog installation obtains a mutation ID before canonical creation`, async (t) => {
    const f = await binaryInstallFixture(editor);
    t.after(() => f.root.close());
    await f.press();
    assert.deepEqual(f.calls, [{ op: 'new_mutation_id' }]);
    assert.deepEqual(f.navigations, []);
    await f.release();
    assert.deepEqual(f.calls[1], {
      op: 'install_app',
      space_id: 'chat',
      app_id: editor,
      args_json: '{"mutationId":"synthetic-mutation"}',
    });
    assert.deepEqual(f.navigations, [
      { pathname: '/miniapp/[id]', params: { id: 'synthetic-created' } },
    ]);
    assert.deepEqual(f.root.errors, []);
  });
}

test('Word catalog refuses a late mutation ID after the account epoch retires', async (t) => {
  const f = await binaryInstallFixture('wordcraft');
  t.after(() => f.root.close());
  await f.press();
  f.store.sessionEpoch += 1;
  await f.release();
  assert.deepEqual(f.calls, [{ op: 'new_mutation_id' }]);
  assert.deepEqual(f.navigations, []);
  assert.ok(f.texts.includes('wordDocuments.accountChanged'));
  assert.deepEqual(f.root.errors, []);
});
