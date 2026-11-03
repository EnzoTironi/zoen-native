import assert from 'node:assert/strict';
import test from 'node:test';
import { pageCreationFixture } from './page-creation-react.fixtures.ts';

for (const imported of [false, true])
  test(`Reader cannot ${imported ? 'import selected Markdown' : 'create a Page'} through the real compiled screen`, async () => {
    const f = await pageCreationFixture('reader');
    try {
      if (imported) await f.pick();
      else await f.title();
      const action = f.button(imported ? 'documents.importAndSharePage' : 'documents.createPage');
      assert.equal(action.disabled, true);
      await f.root.act(() => action.onPress());
      assert.equal(
        f.calls.some((c) => ['page_create', 'page_import_markdown'].includes(c.op)),
        false,
      );
      assert.equal(f.routes.length, 0);
    } finally {
      await f.close();
    }
  });

test('Member imports exact Markdown through the actual native file port and deletes its cache copy', async () => {
  const f = await pageCreationFixture();
  try {
    await f.pick();
    const action = f.button('documents.importAndSharePage');
    assert.equal(action.disabled, false);
    await f.root.act(() => action.onPress());
    assert.equal(f.created.length, 1);
    assert.equal(f.created[0].markdown, f.text);
    assert.equal(f.created[0].space_id, f.room.id);
    assert.equal(f.deleted, 1);
    assert.equal(f.routes.length, 1);
  } finally {
    await f.close();
  }
});

test('pending role read cannot reuse the prior permitted destination callback', async () => {
  const f = await pageCreationFixture();
  try {
    await f.title();
    const old = f.button('documents.createPage');
    const read = f.holdRead();
    await f.refresh();
    assert.equal(f.button('documents.createPage').disabled, true);
    await f.root.act(() => old.onPress());
    assert.equal(f.created.length, 0);
    await f.root.act(() => read.resolve({ ...f.context(), role: 'reader' }));
    assert.equal(f.button('documents.createPage').disabled, true);
  } finally {
    await f.close();
  }
});

test('changing destination while the native file picker is pending retires its old result', async () => {
  const f = await pageCreationFixture();
  try {
    const picker = f.holdPicker();
    await f.pick();
    await f.select();
    await f.root.act(() => picker.resolve());
    assert.equal(f.messages().includes('selected.md'), false);
    assert.equal(f.deleted, 1);
    assert.equal(f.created.length, 0);
  } finally {
    await f.close();
  }
});

for (const patch of [
  { state: 'Updating' },
  { state: 'Removing' },
  { state: 'Removed', member: null },
  { role: null },
  { role: 'unknown' },
  { revision: 'bad' },
  { identity_id: 'other' },
  { device_id: 'other' },
  { space_id: 'other' },
  { member_id: 'other' },
] as const)
  test(`unconfirmed or mismatched destination never enables creation: ${JSON.stringify(patch)}`, async () => {
    const f = await pageCreationFixture('member', 'Group', patch);
    try {
      await f.title();
      const action = f.button('documents.createPage');
      assert.equal(action.disabled, true);
      await f.root.act(() => action.onPress());
      assert.equal(f.created.length, 0);
    } finally {
      await f.close();
    }
  });

test('Reader can keep private imported text and explicitly choose a writable destination', async () => {
  const f = await pageCreationFixture('reader', 'Community');
  try {
    assert.equal(
      f.calls.filter((c) => c.op === 'space_member_context').every((c) => c.space_id === f.room.id),
      true,
    );
    await f.pick();
    assert.equal(f.button('documents.importAndSharePage').disabled, true);
    await f.select();
    const action = f.button('documents.importAndSharePage');
    assert.equal(action.disabled, false);
    await f.root.act(() => action.onPress());
    assert.equal(f.created.length, 1);
    assert.equal(f.created[0].space_id, f.second.id);
    assert.equal(f.created[0].markdown, f.text);
  } finally {
    await f.close();
  }
});

test('Direct retains current membership admission without group member-context fallback', async () => {
  const f = await pageCreationFixture('member', 'Direct');
  try {
    await f.title();
    assert.equal(
      f.calls.some((c) => c.op === 'space_member_context'),
      false,
    );
    const action = f.button('documents.createPage');
    assert.equal(action.disabled, false);
    await f.root.act(() => {
      action.onPress();
      action.onPress();
    });
    assert.equal(f.created.length, 1);
    assert.equal(f.routes.length, 1);
  } finally {
    await f.close();
  }
});

test('a refused role read discards cached permission and requires an explicit read retry', async () => {
  const f = await pageCreationFixture();
  try {
    await f.title();
    const old = f.button('documents.createPage'),
      read = f.holdRead();
    await f.refresh();
    await f.root.act(() => read.reject(Error('context unavailable')));
    assert.equal(f.button('documents.createPage').disabled, true);
    await f.root.act(() => old.onPress());
    assert.equal(f.created.length, 0);
    await f.root.act(() => f.button('documents.tryAgain').onPress());
    assert.equal(
      f.button('documents.createPage').disabled,
      false,
      JSON.stringify({ calls: f.calls, messages: f.messages() }),
    );
  } finally {
    await f.close();
  }
});

test('revocation supersedes a delayed Member read and its old create callback', async () => {
  const f = await pageCreationFixture();
  try {
    await f.title();
    const old = f.button('documents.createPage'),
      read = f.holdRead(),
      permitted = f.context();
    await f.refresh();
    f.roles.set(f.room.id, 'reader');
    await f.refresh();
    await f.root.act(() => read.resolve(permitted));
    await f.root.act(() => old.onPress());
    assert.equal(f.button('documents.createPage').disabled, true);
    assert.equal(f.created.length, 0);
  } finally {
    await f.close();
  }
});

test('selecting another Space supersedes an old role read without rerouting its callback', async () => {
  const f = await pageCreationFixture();
  try {
    await f.title();
    const old = f.button('documents.createPage'),
      read = f.holdRead(),
      permitted = f.context();
    await f.refresh();
    await f.select();
    await f.root.act(() => read.resolve(permitted));
    await f.root.act(() => old.onPress());
    assert.equal(f.created.length, 0);
    await f.root.act(() => f.button('documents.createPage').onPress());
    assert.equal(f.created[0].space_id, f.second.id);
  } finally {
    await f.close();
  }
});

for (const change of ['identity', 'device', 'epoch', 'blur', 'background', 'revocation'] as const)
  test(`late Markdown import cannot attach after ${change}`, async () => {
    const f = await pageCreationFixture();
    try {
      const picker = f.holdPicker();
      await f.pick();
      if (change === 'blur') await f.focus(false);
      else if (change === 'background') await f.background('background');
      else if (change === 'revocation') {
        f.roles.set(f.room.id, 'reader');
        await f.refresh();
      } else await f.replaceOwner(change);
      await f.root.act(() => picker.resolve());
      assert.equal(f.messages().includes('selected.md'), false);
      assert.equal(f.deleted, 1);
      assert.equal(f.created.length, 0);
    } finally {
      await f.close();
    }
  });

test('Core remains authoritative when the role changes after the last permission read', async () => {
  const f = await pageCreationFixture();
  try {
    await f.pick();
    f.roles.set(f.room.id, 'reader'); // No UI event yet; the Core boundary already knows.
    await f.root.act(() => f.button('documents.importAndSharePage').onPress());
    assert.equal(f.created.length, 0);
    assert.equal(f.routes.length, 0);
    assert.equal(f.calls.filter((c) => c.op === 'page_import_markdown').length, 1);
    assert.equal(
      f.button('documents.importAndSharePage').disabled,
      true,
      JSON.stringify({ calls: f.calls, messages: f.messages() }),
    );
  } finally {
    await f.close();
  }
});

for (const change of ['identity', 'device', 'epoch'] as const)
  test(`late permission response and callback cannot authorize after ${change}`, async () => {
    const f = await pageCreationFixture();
    try {
      await f.title();
      const old = f.button('documents.createPage'),
        read = f.holdRead(),
        before = f.context();
      await f.refresh();
      await f.replaceOwner(change);
      await f.root.act(() => read.resolve(before));
      await f.root.act(() => old.onPress());
      assert.equal(f.created.length, 0);
      await f.title('New session Page');
      assert.equal(f.button('documents.createPage').disabled, false);
    } finally {
      await f.close();
    }
  });

test('a retired focus callback cannot revive when the same route focuses again', async () => {
  const f = await pageCreationFixture();
  try {
    await f.title();
    const old = f.button('documents.createPage');
    await f.focus(false);
    await f.focus(true);
    await f.root.act(() => old.onPress());
    assert.equal(f.created.length, 0);
    await f.root.act(() => f.button('documents.createPage').onPress());
    assert.equal(f.created.length, 1);
  } finally {
    await f.close();
  }
});

test('revocation during the actual Markdown file read discards the late text and cleans its copy', async () => {
  const f = await pageCreationFixture();
  try {
    const read = f.holdFileRead();
    await f.pick();
    assert.equal(f.deleted, 0);
    f.roles.set(f.room.id, 'reader');
    await f.refresh();
    await f.root.act(() => read.resolve());
    assert.equal(f.messages().includes('selected.md'), false);
    assert.equal(f.deleted, 1);
    assert.equal(f.created.length, 0);
  } finally {
    await f.close();
  }
});

test('foreground without route blur requires a fresh role before re-enabling creation', async () => {
  const f = await pageCreationFixture();
  try {
    await f.title();
    const old = f.button('documents.createPage');
    await f.background('background');
    f.roles.set(f.room.id, 'reader');
    const read = f.holdRead();
    await f.background('active');
    assert.equal(f.button('documents.createPage').disabled, true);
    await f.root.act(() => old.onPress());
    await f.root.act(() => read.resolve(f.context()));
    assert.equal(f.button('documents.createPage').disabled, true);
    assert.equal(f.created.length, 0);
  } finally {
    await f.close();
  }
});

for (const phase of ['picker', 'file-read'] as const)
  test(`unrelated canonical refresh preserves the selected local file during ${phase}`, async () => {
    const f = await pageCreationFixture();
    try {
      const pending = phase === 'picker' ? f.holdPicker() : f.holdFileRead();
      await f.pick();
      await f.refresh();
      await f.root.act(() => pending.resolve());
      assert.equal(f.messages().includes('selected.md'), true);
      assert.equal(f.button('documents.importAndSharePage').disabled, false);
      assert.equal(f.created.length, 0, 'local selection never publishes a document');
      assert.equal(f.deleted, 1);
    } finally {
      await f.close();
    }
  });

test('a permission change followed by restoration still retires the old file picker', async () => {
  const f = await pageCreationFixture();
  try {
    const pending = f.holdPicker();
    await f.pick();
    f.roles.set(f.room.id, 'reader');
    await f.refresh();
    f.roles.set(f.room.id, 'member');
    await f.refresh();
    await f.root.act(() => pending.resolve());
    assert.equal(f.messages().includes('selected.md'), false);
    assert.equal(f.created.length, 0);
    assert.equal(f.deleted, 1);
  } finally {
    await f.close();
  }
});
