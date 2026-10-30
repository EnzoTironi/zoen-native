import assert from 'node:assert/strict';
import { test } from 'node:test';
import { messageDraftReactFixture } from './message-draft-react.fixtures.ts';
import { entry as stubEntry } from './store.fixtures.ts';
const entry = (id: string) => ({
  ...stubEntry(id),
  kind: { Message: { text: 'Original', card: null } },
  voice: null,
});

for (const fails of [false, true])
  test(`compiled edit carries the opened revision and preserves the normal draft: ${fails ? 'storage refusal' : 'accepted'}`, async () => {
    const f = await messageDraftReactFixture();
    try {
      await f.render();
      const original = f.current().messageDraft.text;
      const normalOperation = f.driver.operation;
      f.driver.operation = async (command) => {
        if (command.op !== 'send_message_edit_draft') return normalOperation(command);
        if (fails) {
          f.calls.push(structuredClone(command));
          throw new Error('storage refused');
        }
        return normalOperation(command);
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
      const saved = f.calls.filter((c) => c.op === 'save_message_draft').at(-1);
      assert.deepEqual(saved.edit, {
        message_id: 'source',
        source_revision: 'opened-revision',
        base_text: 'Original text',
        text: 'User correction 😀',
        is_caption: false,
      });
      const request = f.calls.find((c) => c.op === 'send_message_edit_draft');
      assert.deepEqual(request, {
        op: 'send_message_edit_draft',
        expected_identity_id: f.snapshot.account!.identity_id,
        expected_device_id: f.snapshot.account!.device_id,
        space_id: f.snapshot.conversations[0]!.id,
        thread_root_id: null,
        expected_revision: String(Number(saved.expected_revision) + 1),
      });
      assert.equal(f.current().messageDraft.text, original);
      assert.equal(f.current().messageDraft.editText, fails ? 'User correction 😀' : '');
      assert.equal(f.calls.filter((c) => c.op === 'send_message_draft').length, 0);
      assert.equal(f.current().messageDraft.text, original);
      assert.equal(Boolean(f.current().messageDraft.editing), fails);
      // Close may drain the same explicitly requested, still ambiguous edit.
      f.driver.operation = normalOperation;
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
