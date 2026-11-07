import assert from 'node:assert/strict';
import { test } from 'node:test';
import { liveMessageActions } from './message-actions-live.fixtures.ts';
import { conversationScope } from './conversation-scope.ts';
import { deferred } from './store.fixtures.ts';

for (const boundary of [
  'reopen',
  'relay',
  'access-aba',
  'focus-aba',
  'disabled',
  'thread-aba',
] as const) {
  test(`selected-message controls retire after ${boundary} and a fresh selection works`, async () => {
    const f = await liveMessageActions();
    try {
      await f.root.act(() => f.selection().toggle(f.entry));
      const old = f.selection();
      assert.equal(old.count, 1);
      await f.change(boundary);
      await f.root.act(() => {
        old.copy();
        old.forward();
        old.remove();
        old.toggle(f.entries[1]!);
      });
      assert.deepEqual(f.copies, []);
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.commands, []);
      assert.deepEqual(f.confirmations, []);
      assert.equal(f.selection().count, 0);
      await f.root.act(() => f.selection().toggle(f.entry));
      assert.equal(f.selection().count, 1);
      await f.root.act(() => f.selection().copy());
      assert.deepEqual(f.copies, ['Synthetic author-navigation fixture']);
      await f.root.act(() => {
        old.cancel();
        old.toggle(f.entries[1]!);
      });
      assert.equal(f.selection().count, 1);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.close();
    }
  });
}

for (const boundary of ['access-aba', 'focus-aba', 'disabled', 'thread-aba'] as const) {
  test(`an old unselected-row callback cannot start selection after ${boundary}`, async () => {
    const f = await liveMessageActions();
    try {
      const old = f.selection();
      await f.change(boundary);
      await f.root.act(() => old.toggle(f.entry));
      assert.equal(f.selection().count, 0);
      await f.root.act(() => f.selection().toggle(f.entry));
      assert.equal(f.selection().count, 1);
    } finally {
      await f.close();
    }
  });
}

test('bulk copy and forwarding use current canonical order and carry identities, not message content', async () => {
  const f = await liveMessageActions();
  try {
    await f.root.act(() => f.selection().toggle(f.entries[1]!));
    await f.root.act(() => f.selection().toggle(f.entry));
    assert.equal(f.selection().count, 2);
    await f.root.act(() => {
      f.selection().copy();
      f.selection().forward();
    });
    assert.deepEqual(f.copies, ['Synthetic author-navigation fixture\n\nSecond message']);
    assert.deepEqual(structuredClone(f.routes), [
      {
        pathname: '/forward-selection/[space]',
        params: {
          space: 'read-chat',
          messages: JSON.stringify(f.entries.map((entry) => entry.id)),
          selectionScope: conversationScope(
            f.store.getSnapshot(),
            f.store.sessionEpoch,
            'read-chat',
          ),
        },
      },
    ]);
    assert.deepEqual(f.commands, []);
  } finally {
    await f.close();
  }
});

test('bulk deletion confirms once and sends exact native identity and source revisions', async () => {
  const f = await liveMessageActions();
  try {
    const entries = f.entries;
    const account = f.store.getSnapshot().snapshot!.account!;
    await f.root.act(() => {
      f.selection().toggle(entries[1]!);
      f.selection().toggle(entries[0]!);
    });
    f.choose(async () => 'everyone');
    await f.root.act(() => {
      const selected = f.selection();
      selected.remove();
      selected.remove();
    });
    assert.equal(f.confirmations.length, 1);
    assert.deepEqual(structuredClone(f.commands), [
      {
        op: 'delete_messages',
        for_me: false,
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: 'read-chat',
        targets: entries.map((entry) => ({
          message_id: entry.id,
          expected_revision: entry.text_revision,
        })),
      },
    ]);
    assert.equal(f.selection().count, 0);
    assert.equal(f.selection().pending, false);
  } finally {
    await f.close();
  }
});

test('an outstanding bulk-deletion confirmation cannot survive access loss and restoration', async () => {
  const f = await liveMessageActions();
  const answer = deferred<string | null>();
  try {
    await f.root.act(() => f.selection().toggle(f.entry));
    f.choose(() => answer.promise);
    await f.root.act(() => f.selection().remove());
    assert.equal(f.confirmations.length, 1);
    await f.change('access-aba');
    await f.root.act(() => {
      answer.resolve('everyone');
    });
    assert.deepEqual(f.commands, []);
    assert.equal(f.selection().count, 0);
    assert.equal(f.selection().pending, false);
  } finally {
    answer.resolve(null);
    await f.close();
  }
});

test('cancelled and failed bulk deletion keep the selection and require another explicit confirmation', async () => {
  const f = await liveMessageActions();
  try {
    await f.root.act(() => f.selection().toggle(f.entry));
    f.choose(async () => null);
    await f.root.act(() => f.selection().remove());
    assert.equal(f.selection().count, 1);
    assert.deepEqual(f.commands, []);
    f.choose(async () => 'for_me');
    f.failMutation(true);
    await f.root.act(() => f.selection().remove());
    assert.equal(f.selection().count, 1);
    assert.equal(f.selection().pending, false);
    f.failMutation(false);
    await f.root.act(() => f.selection().remove());
    assert.equal(f.confirmations.length, 3);
    assert.equal(f.commands.length, 2);
    assert.equal(f.selection().count, 0);
  } finally {
    await f.close();
  }
});
