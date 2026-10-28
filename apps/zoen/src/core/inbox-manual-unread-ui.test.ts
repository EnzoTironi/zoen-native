import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chatSearchReactFixture } from './chat-search-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import type { TimelinePage } from './generated/TimelinePage';
import { inboxUi } from './inbox-ui.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: a fully read conversation offers marking unread through the actual leading swipe`, async () => {
    const f = await inboxUi(locale);
    try {
      f.core.chat.unread = 0;
      await f.act(() => f.store.refresh());
      await f.reveal(f.core.chat.id, 'leading');
      const action = f.control(`inbox-${f.core.chat.id}-read`);
      assert.equal(action.accessibilityLabel, f.t('inboxActions.unread'));
      await f.act(() => action.onPress());
      assert.equal(f.core.chat.manual_unread, true);
      assert.equal(f.core.chat.unread, 0, 'Manual unread does not manufacture unread messages');
      assert.ok(f.has(`inbox-unread-${f.core.chat.id}`));
      assert.equal(
        f.control(`inbox-chat-${f.core.chat.id}`).accessibilityLabel,
        f.t('inboxActions.manuallyUnreadLabel', {
          title: f.core.chat.title,
          preview: f.core.chat.last_preview,
        }),
      );
      await f.act(() => f.reopen());
      await f.act(() => f.control('inbox-filter-unread').onPress());
      assert.ok(f.has(`inbox-chat-${f.core.chat.id}`));
      const menu = f.control(`inbox-menu-${f.core.chat.id}`);
      assert.equal(
        menu.actions.find((item: any) => item.id === 'read').title,
        f.t('inboxActions.read'),
      );
      await f.act(() => menu.onPressAction({ nativeEvent: { event: 'read' } }));
      assert.equal(f.core.chat.manual_unread, false);
      assert.equal(f.has(`inbox-chat-${f.core.chat.id}`), false);
      assert.equal(f.has('inbox-undo'), false, 'Read marking does not replace layout Undo');
    } finally {
      await f.close();
    }
  });

test('opening an empty manually unread conversation waits for its canonical page before clearing the mark', async () => {
  const f = await chatSearchReactFixture();
  try {
    const page = deferred<TimelinePage>();
    f.timeline(() => page.promise);
    f.snapshot.conversations[0]!.manual_unread = true;
    await f.act(() => f.store.refresh());
    await f.render();
    assert.equal(f.calls.filter((command) => command.op === 'mark_read').length, 0);
    await f.act(() => page.resolve({ entries: [], before: null, has_more: false }));
    assert.equal(f.calls.filter((command) => command.op === 'mark_read').length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
