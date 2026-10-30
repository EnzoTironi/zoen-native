import assert from 'node:assert/strict';
import { test } from 'node:test';
import { messageDraftReactFixture } from './message-draft-react.fixtures.ts';
import { entry } from './store.fixtures.ts';

for (const fails of [false, true])
  test(`compiled edit carries the opened revision and preserves the normal draft: ${fails ? 'storage refusal' : 'accepted'}`, async () => {
    const f = await messageDraftReactFixture();
    try {
      await f.render();
      const original = f.current().messageDraft.text;
      const normalOperation = f.driver.operation;
      f.driver.operation = async (command) => {
        if (command.op !== 'edit_message') return normalOperation(command);
        f.calls.push(structuredClone(command));
        if (fails) throw new Error('storage refused');
        return null;
      };
      await f.act(() =>
        f.current().updateDraft({
          type: 'edit',
          entry: { ...entry('source'), text_revision: 'opened-revision' },
          text: 'Original text',
        }),
      );
      await f.act(() => f.current().updateDraft({ type: 'text', value: 'User correction 😀' }));
      const attempt = f.current().capture();
      assert.ok(attempt);
      assert.equal(attempt.editingRevision, 'opened-revision');
      if (fails)
        await f.act(() => assert.rejects(f.current().sendEdit(attempt), /storage refused/));
      else await f.act(() => f.current().sendEdit(attempt));
      const request = f.calls.find((c) => c.op === 'edit_message');
      assert.deepEqual(request, {
        op: 'edit_message',
        expected_identity_id: f.snapshot.account!.identity_id,
        expected_device_id: f.snapshot.account!.device_id,
        space_id: f.snapshot.conversations[0]!.id,
        message_id: 'source',
        expected_revision: 'opened-revision',
        text: 'User correction 😀',
      });
      assert.equal(f.current().messageDraft.text, original);
      assert.equal(f.current().messageDraft.editText, 'User correction 😀');
      assert.equal(f.calls.filter((c) => c.op === 'send_message_draft').length, 0);
      if (!fails) await f.act(() => f.current().updateDraft({ type: 'accepted', attempt }));
      assert.equal(f.current().messageDraft.text, original);
      assert.equal(Boolean(f.current().messageDraft.editing), fails);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('retired editor cannot send its captured correction in a new account', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: { ...entry('source'), text_revision: 'r1' },
        text: 'Private correction',
      }),
    );
    const old = f.current(),
      attempt = old.capture();
    assert.ok(attempt);
    await f.reopen('identity');
    const count = f.calls.length;
    await assert.rejects(old.sendEdit(attempt), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });
    assert.equal(f.calls.length, count);
    assert.equal(old.isCurrent(), false);
    assert.equal(f.current().messageDraft.text, 'New session canonical draft');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('a failure from a replaced editor cannot claim the current composer', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: { ...entry('first'), text_revision: 'r1' },
        text: 'First correction',
      }),
    );
    const old = f.current(),
      attempt = old.capture();
    assert.ok(attempt);
    assert.equal(old.isCurrent(attempt), true);
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: { ...entry('second'), text_revision: 'r2' },
        text: 'New correction',
      }),
    );
    assert.equal(old.isCurrent(attempt), false);
    assert.equal(f.current().isCurrent(), true);
    const count = f.calls.length;
    await assert.rejects(old.sendEdit(attempt));
    assert.equal(f.calls.length, count);
    assert.equal(f.current().messageDraft.editText, 'New correction');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled reopen reviews the new source revision while retaining the existing correction', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: { ...entry('source'), text_revision: 'r1' },
        text: 'Original',
      }),
    );
    await f.act(() => f.current().updateDraft({ type: 'text', value: 'Keep my correction' }));
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: { ...entry('source'), text_revision: 'r2' },
        text: 'Freshly selected source',
      }),
    );
    const attempt = f.current().capture();
    assert.equal(attempt?.editingRevision, 'r2');
    assert.equal(attempt?.text, 'Keep my correction');
    assert.equal(f.current().messageDraft.text, 'Owner A private unsent text');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
