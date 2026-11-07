import assert from 'node:assert/strict';
import { test } from 'node:test';
import { composerProps } from './composer-recovery-screen.fixtures.ts';
import { messageDraftReactFixture } from './message-draft-react.fixtures.ts';
import { entry } from './store.fixtures.ts';

for (const surface of ['chat', 'thread'] as const) {
  test(`${surface} exposes the same durable recovery state while editing and retains both drafts`, async () => {
    const f = await messageDraftReactFixture(true, surface === 'thread' ? 'root-message' : null);
    const operation = f.driver.operation;
    try {
      await f.render();
      const normal = f.current().messageDraft.text;
      let fail = true;
      f.driver.operation = async (command) => {
        if (command.op === 'save_message_draft' && fail) throw Error('private storage unavailable');
        return operation(command);
      };
      const source = {
        ...entry('source'),
        text_revision: 'reviewed-source',
        kind: { Message: { text: 'Original', card: null } },
        voice: null,
      };
      await f.act(() => f.current().updateDraft({ type: 'edit', entry: source, text: 'Original' }));
      await f.act(() => f.current().updateDraft({ type: 'text', value: 'Correction 😀' }));
      await f.act(() => f.current().flush());
      assert.ok(f.current().error);
      const props = composerProps(surface, f.current());
      assert.equal(props.editing, true);
      assert.equal(props.value, 'Correction 😀');
      assert.ok(props.draft?.error, 'editing must not hide the save failure');
      assert.equal(props.draft.ready, true, 'loaded draft remains editable during recovery');
      fail = false;
      await f.act(() => props.draft.retry());
      assert.equal(f.current().error, null);
      const saved = f.calls.filter((c) => c.op === 'save_message_draft').at(-1);
      assert.equal(saved.text, normal);
      assert.equal(saved.edit.text, 'Correction 😀');
      assert.equal(saved.thread_root_id, surface === 'thread' ? 'root-message' : null);
      await f.reopen(undefined, true);
      const restored = composerProps(surface, f.current());
      assert.equal(restored.value, 'Correction 😀');
      assert.equal(restored.context.edit.revision, 'reviewed-source');
      assert.equal(f.current().messageDraft.text, normal);
      await f.act(() => restored.context.onCancel());
      assert.equal(composerProps(surface, f.current()).value, normal);
      assert.equal(f.current().messageDraft.editing, null);
      assert.equal(f.calls.filter((c) => c.op.startsWith('send_')).length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      f.driver.operation = operation;
      await f.close();
    }
  });
}

test('search keeps draft recovery attached to the normal composer instead of the search field', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    assert.equal(composerProps('chat', f.current(), true).draft, undefined);
    assert.equal(composerProps('chat', f.current(), false).draft.ready, true);
  } finally {
    await f.close();
  }
});
