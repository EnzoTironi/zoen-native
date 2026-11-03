import assert from 'node:assert/strict';
import { test } from 'node:test';
import { calendarUi } from './conversation-date-ui.fixtures.ts';
import { localMessageDay } from './conversation-date.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: compiled native calendar changes day, reads exactly that scope and waits for reveal`, async () => {
    const f = await calendarUi(locale);
    try {
      await f.root.act(() => f.control('chat-calendar-search').onPress());
      assert.equal(
        f.control('chat-calendar-search').label,
        locale === 'en' ? 'Jump to Date' : 'Ir para data',
      );
      assert.equal(f.opened.length, 1);
      assert.equal(f.control('sheet').isPresented, true);
      const picker = f.control('chat-calendar-day');
      assert.equal(picker.mode, 'date');
      assert.equal(picker.display, 'inline');
      const chosen = new Date(2026, 9, 17, 15);
      await f.root.act(() => picker.onValueChange({}, chosen));
      await f.root.act(() => f.control('chat-calendar-go').onPress());
      assert.equal(f.control('chat-calendar-go').disabled, true);
      assert.equal(f.calls.length, 1);
      assert.equal(f.calls[0].op, 'message_date_target');
      assert.equal(f.calls[0].scope.space_id, f.id);
      assert.equal(f.calls[0].scope.thread_root_id, null);
      assert.equal(f.calls[0].start_ms, localMessageDay(chosen).start_ms);
      assert.equal(f.calls[0].end_ms, localMessageDay(chosen).end_ms);
      await f.root.act(() => f.response.resolve('real-message'));
      assert.deepEqual(f.revealed, ['real-message']);
      assert.deepEqual(f.selected, []);
      await f.root.act(() => f.reveal.resolve(undefined));
      assert.deepEqual(f.selected, ['real-message']);
      assert.equal(f.control('sheet').isPresented, false);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.close();
    }
  });

test('compiled calendar keeps its day and offers retry when the selected message can no longer be revealed', async () => {
  const f = await calendarUi();
  try {
    await f.root.act(() => f.control('chat-calendar-search').onPress());
    await f.root.act(() => f.control('chat-calendar-go').onPress());
    await f.root.act(() => f.response.resolve('deleted-during-read'));
    await f.root.act(() => f.reveal.reject(Error('private fixture details')));
    assert.equal(f.control('sheet').isPresented, true);
    assert.equal(f.control('chat-calendar-go').disabled, false);
    assert.equal(f.control('text').accessibilityRole, 'alert');
    assert.equal(f.control('text').children, 'Could not open this day. Try again.');
    assert.deepEqual(f.selected, []);
  } finally {
    await f.close();
  }
});

for (const retirement of ['dismiss', 'background', 'revoke'] as const)
  test(`${retirement}: compiled sheet cannot navigate when its pending read completes`, async () => {
    const f = await calendarUi();
    try {
      await f.root.act(() => f.control('chat-calendar-search').onPress());
      await f.root.act(() => f.control('chat-calendar-go').onPress());
      if (retirement === 'dismiss') await f.root.act(() => f.control('sheet').onDismiss());
      else await f[retirement]();
      await f.root.act(() => f.response.resolve('old-message'));
      assert.deepEqual(f.selected, []);
      assert.deepEqual(f.revealed, []);
      assert.deepEqual(f.root.errors, []);
    } finally {
      await f.close();
    }
  });
