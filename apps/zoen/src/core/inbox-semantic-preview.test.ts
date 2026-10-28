import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inboxUi } from './inbox-ui.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: inbox uses one semantic projection for its visible preview and accessibility`, async () => {
    const f = await inboxUi(locale);
    try {
      const chat = f.core.chat;
      const source = '> **café🦊**\n> [Read more](https://example.org/a_(b))';
      chat.last_preview = source;
      chat.last_preview_is_markdown = true;
      await f.act(() => f.store.refresh([chat.id]));
      assert.equal(f.control(`inbox-preview-${chat.id}`).children, '…');
      assert.ok(!f.control(`inbox-chat-${chat.id}`).accessibilityLabel.includes('**'));
      assert.equal(f.projections.at(-1)!.markdown, source, 'parse the complete canonical body');
      await f.act(() => f.projections.at(-1)!.result.resolve('café🦊\nRead more'));
      assert.equal(f.control(`inbox-preview-${chat.id}`).children, 'café🦊\nRead more');
      assert.equal(f.control(`inbox-preview-${chat.id}`).numberOfLines, 2);
      assert.equal(
        f.control(`inbox-chat-${chat.id}`).accessibilityLabel,
        f.t('chat.conversationUnreadLabel', {
          title: chat.title,
          count: chat.unread,
          preview: 'café🦊\nRead more',
        }),
      );
      assert.equal(chat.last_preview, source, 'presentation cannot rewrite the canonical body');
      assert.equal(f.calls.filter((call) => call.op === 'message').length, 0);
      await f.act(() => f.control(`inbox-chat-${chat.id}`).onPress());
      assert.ok(JSON.stringify(f.routes).includes(chat.id), 'opening keeps the exact destination');
    } finally {
      await f.close();
    }
  });

test('generated labels remain literal even when filenames contain Markdown punctuation', async () => {
  const f = await inboxUi();
  try {
    const chat = f.core.chat;
    chat.last_preview = 'File · **report_[1]**.pdf';
    chat.last_preview_is_markdown = false;
    await f.act(() => f.store.refresh([chat.id]));
    assert.equal(f.projections.length, 0);
    assert.equal(f.control(`inbox-preview-${chat.id}`).children, chat.last_preview);
    assert.ok(f.control(`inbox-chat-${chat.id}`).accessibilityLabel.includes(chat.last_preview));
    chat.last_preview = 'ordinary café🦊 text';
    chat.last_preview_is_markdown = true;
    await f.act(() => f.store.refresh([chat.id]));
    assert.equal(f.projections.length, 0, 'plain text needs no native parser round trip');
    assert.equal(f.control(`inbox-preview-${chat.id}`).children, chat.last_preview);
  } finally {
    await f.close();
  }
});

test('edited and deleted previews retire pending projections without reviving their old content', async () => {
  const f = await inboxUi();
  try {
    const chat = f.core.chat;
    chat.last_preview = '**old**';
    chat.last_preview_is_markdown = true;
    await f.act(() => f.store.refresh([chat.id]));
    const old = f.projections.at(-1)!;
    chat.last_preview = '*edited*';
    await f.act(() => f.store.refresh([chat.id]));
    await f.act(() => old.result.resolve('old'));
    assert.equal(f.control(`inbox-preview-${chat.id}`).children, '…');
    const edited = f.projections.at(-1)!;
    chat.last_preview = 'Message deleted';
    chat.last_preview_is_markdown = false;
    await f.act(() => f.store.refresh([chat.id]));
    await f.act(() => edited.result.resolve('edited'));
    assert.equal(f.control(`inbox-preview-${chat.id}`).children, 'Message deleted');
    assert.ok(f.control(`inbox-chat-${chat.id}`).accessibilityLabel.includes('Message deleted'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
