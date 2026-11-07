import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred } from './store.fixtures.ts';
import { liveMessageActions } from './message-actions-live.fixtures.ts';

for (const change of [
  'reopen',
  'relay',
  'access-aba',
  'focus-aba',
  'disabled',
  'thread-aba',
] as const) {
  test(`visible message controls retire after ${change}, and fresh controls still work`, async () => {
    const f = await liveMessageActions();
    try {
      await f.open();
      const old = f.panel();
      assert.ok(old.entry);
      await f.change(change);
      await f.root.act(() => {
        old.onReply(f.entry);
        old.onThread(f.entry);
        old.onForward?.(f.entry);
        old.onCopy?.(f.entry);
        old.onEdit?.(f.entry);
        old.onReact('❤️');
      });
      assert.equal(f.replies.length, 0);
      assert.equal(f.threads.length, 0);
      assert.equal(f.routes.length, 0);
      assert.equal(f.copies.length, 0);
      assert.equal(f.commands.length, 0);
      assert.equal(f.edits.length, 0);
      await f.open();
      const fresh = f.panel();
      assert.ok(fresh.entry);
      await f.root.act(() => fresh.onReply(f.entry));
      assert.equal(f.replies.length, 1);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.close();
    }
  });
}

for (const action of ['thread', 'copy', 'forward', 'react', 'edit'] as const) {
  test(`the current compiled message menu performs ${action} through the canonical boundary`, async () => {
    const f = await liveMessageActions();
    try {
      await f.open();
      const panel = f.panel();
      await f.root.act(() => {
        if (action === 'thread') panel.onThread(f.entry);
        if (action === 'copy') panel.onCopy?.(f.entry);
        if (action === 'forward') panel.onForward?.(f.entry);
        if (action === 'react') panel.onReact('❤️');
        if (action === 'edit') panel.onEdit?.(f.entry);
      });
      assert.equal(f.threads.length, action === 'thread' ? 1 : 0);
      assert.equal(f.copies.length, action === 'copy' ? 1 : 0);
      assert.equal(f.routes.length, action === 'forward' ? 1 : 0);
      assert.equal(f.commands.length, action === 'react' ? 1 : 0);
      assert.equal(f.edits.length, action === 'edit' ? 1 : 0);
      if (action === 'react')
        assert.deepEqual(structuredClone(f.commands[0]), {
          op: 'react',
          space_id: 'read-chat',
          message_id: f.entry.id,
          emoji: '❤️',
        });
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.close();
    }
  });
}

test('a single-message deletion confirmation retires when its thread view changes', async () => {
  const f = await liveMessageActions();
  const answer = deferred<string | null>();
  try {
    await f.open();
    f.choose(() => answer.promise);
    await f.root.act(() => f.panel().onDelete?.(f.entry));
    assert.equal(f.confirmations.length, 1);
    await f.change('thread-aba');
    assert.equal(f.confirmations[0]!.signal?.aborted, true);
    await f.root.act(() => {
      answer.resolve('everyone');
    });
    assert.deepEqual(f.commands, []);
    await f.open();
    assert.ok(f.panel().entry);
    assert.deepEqual(f.root.errors, []);
  } finally {
    answer.resolve(null);
    await f.close();
  }
});
