import assert from 'node:assert/strict';
import { test } from 'node:test';
import { folderUi } from './inbox-folder-settings.fixtures.ts';
import { deferred } from './store.fixtures.ts';

test('actual folder settings create, name, choose rules and persist exactly one owned configuration', async () => {
  const f = await folderUi();
  try {
    await f.press('folder-add');
    await f.name('Trabalho 🪁');
    await f.root.act(() => f.control('folder-kind-Group').onValueChange(true));
    await f.root.act(() => f.control(`folder-chat-${f.chat.id}`).onValueChange(true));
    await f.press('folder-save');
    const writes = f.calls.filter((c) => c.op === 'inbox_folders');
    assert.equal(writes.length, 1);
    assert.equal(writes[0].input.observed_revision, '0');
    assert.deepEqual(writes[0].input.configuration.folders[0], {
      id: 'folder:intent-1',
      name: 'Trabalho 🪁',
      kinds: ['Group'],
      include: [f.chat.id],
      exclude: [],
      exclude_read: false,
      exclude_muted: false,
      exclude_archived: true,
    });
    assert.equal(f.control('folder-save').disabled, true);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('lost ACK freezes editing and retries the exact same operation and configuration', async () => {
  const f = await folderUi();
  try {
    await f.press('folder-add');
    await f.name('Files');
    f.loseAck();
    await f.press('folder-save');
    assert.equal(f.control('folder-name').editable, false);
    assert.equal(f.control('folder-kind-Group').disabled, true);
    await f.press('folder-save');
    const writes = f.calls.filter((c) => c.op === 'inbox_folders');
    assert.equal(writes.length, 2);
    assert.deepEqual(writes[0], writes[1]);
    assert.equal(f.snapshot.inbox_folders?.configuration.folders[0].name, 'Files');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('account change while obtaining a mutation ID prevents the old draft from being written', async () => {
  const f = await folderUi();
  try {
    await f.press('folder-add');
    await f.name('Private folder');
    const id = deferred<string>();
    f.nextId(() => id.promise);
    await f.press('folder-save');
    await f.root.act(async () => {
      if (f.snapshot.account) f.snapshot.account.identity_id = 'other-owner';
      await f.store.refresh();
    });
    await f.root.act(async () => {
      id.resolve('delayed');
      await id.promise;
    });
    assert.equal(
      f.calls.some((c) => c.op === 'inbox_folders'),
      false,
    );
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('default folder, reorder and deletion are one draft; discard never writes it', async () => {
  const f = await folderUi();
  try {
    await f.press('folder-add');
    await f.name('One');
    await f.press('Back');
    await f.press('folder-add');
    await f.name('Two');
    await f.press('Move earlier');
    await f.press('Back');
    await f.root.act(() => f.control('folder-initial').onValueChange('folder:intent-2'));
    await f.press('folder-save');
    const configuration = f.snapshot.inbox_folders?.configuration;
    assert.equal(configuration?.default_folder, 'folder:intent-2');
    assert.deepEqual(
      configuration?.folders.map((folder) => folder.name),
      ['Two', 'One'],
    );
    await f.press('folder-edit-folder:intent-2');
    await f.press('Delete folder');
    assert.equal(f.control('folder-initial').selectedValue, 'all');
    await f.press('folder-discard');
    assert.equal(f.control('folder-initial').selectedValue, 'folder:intent-2');
    assert.equal(f.calls.filter((c) => c.op === 'inbox_folders').length, 1);
  } finally {
    await f.close();
  }
});
