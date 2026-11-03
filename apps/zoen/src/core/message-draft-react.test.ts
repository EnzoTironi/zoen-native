import assert from 'node:assert/strict';
import { test } from 'node:test';
import { messageDraftReactFixture } from './message-draft-react.fixtures.ts';

test('actual compiled composer attaches its durable draft after initial account open', async () => {
  const f = await messageDraftReactFixture(false);
  try {
    await f.render();
    assert.equal(f.current().ready, false);
    await f.open();
    assert.equal(f.current().ready, true);
    assert.equal(f.current().capture()?.text, 'Owner A private unsent text');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const change of [undefined, 'identity', 'device', 'relay'] as const)
  test(`actual compiled composer retires old channel after close/open: ${change ?? 'same IDs/new epoch'}`, async () => {
    const f = await messageDraftReactFixture();
    try {
      await f.render();
      assert.equal(f.current().capture()?.text, 'Owner A private unsent text');
      const old = f.current();
      await f.reopen(change);
      assert.equal(
        f.current().messageDraft.text,
        'New session canonical draft',
        'old owner/session text must not remain visible',
      );
      assert.equal(f.current().ready, true);
      assert.equal(f.current().capture()?.text, 'New session canonical draft');
      const before = f.calls.length;
      await f.act(() => old.updateDraft({ type: 'text', value: 'stale callback' }));
      assert.equal(old.capture(), null);
      await assert.rejects(old.send(), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });
      assert.equal(f.calls.length, before);
      assert.equal(f.current().capture()?.text, 'New session canonical draft');
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('compiled composer persists exact current text once and restores it after retirement', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    const value = '  Café 🌤️\r\nStill private and unsent  ';
    await f.act(() => f.current().updateDraft({ type: 'text', value }));
    await f.act(() => f.current().flush());
    assert.equal(f.calls.filter((c) => c.op === 'save_message_draft').length, 1);
    await f.reopen(undefined, true);
    assert.equal(f.current().capture()?.text, value);
    assert.equal(f.current().messageDraft.text, value);
    assert.equal(f.calls.filter((c) => c.op === 'save_message_draft').length, 1);
    assert.equal(f.calls.filter((c) => c.op === 'send_message_draft').length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled composer hides text and refuses old controls on canonical conversation loss', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    const old = f.current();
    assert.equal(old.messageDraft.text, 'Owner A private unsent text');
    f.snapshot.conversations = [];
    await f.act(() => f.store.refresh());
    assert.equal(f.current().messageDraft.text, '');
    assert.equal(f.current().ready, false);
    const count = f.calls.length;
    await f.act(() => old.updateDraft({ type: 'text', value: 'revoked callback' }));
    assert.equal(old.capture(), null);
    await assert.rejects(old.send(), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });
    assert.equal(f.calls.length, count);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
