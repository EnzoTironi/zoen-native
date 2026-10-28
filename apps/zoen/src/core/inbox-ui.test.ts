import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inboxUi } from './inbox-ui.fixtures.ts';
import { deferred } from './store.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: actual Chats trailing archive waits for canonical ACK, exposes Undo and restores the row without changing finite mute`, async () => {
    const f = await inboxUi(locale);
    try {
      const id = f.core.chat.id;
      assert.ok(f.has(`inbox-chat-${id}`));
      await f.reveal(id, 'trailing');
      assert.equal(
        f.calls.some((call) => call.op === 'chat_layout' && call.input.kind === 'change'),
        false,
        'Revealing actions is not a mutation',
      );
      assert.equal(
        f.control(`inbox-${id}-archive`).accessibilityLabel,
        f.t('inboxActions.archive'),
      );
      await f.act(() => f.control(`inbox-${id}-archive`).onPress());
      assert.equal(f.has(`inbox-chat-${id}`), false);
      assert.equal(f.has('inbox-undo'), true);
      assert.equal(f.core.chat.muted_until_ms, '1900000000000');
      const change = f.calls.find((c) => c.op === 'chat_layout' && c.input.kind === 'change').input;
      assert.equal(change.field, 'archived');
      assert.equal(change.value, true);
      await f.act(() => f.control('inbox-undo').onPress());
      assert.equal(f.has(`inbox-chat-${id}`), true);
      assert.equal(f.core.chat.muted_until_ms, '1900000000000');
      const undo = f.calls.find((c) => c.op === 'chat_layout' && c.input.kind === 'undo').input;
      assert.equal(undo.original_operation_id, change.operation_id);
      assert.notEqual(undo.operation_id, change.operation_id);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('archive does not disappear optimistically before its canonical response', async () => {
  const f = await inboxUi();
  try {
    const ack = deferred<any>();
    f.layout((input) => (input.kind === 'change' ? ack.promise : f.core.canonical(input)));
    await f.reveal(f.core.chat.id, 'trailing');
    await f.act(() => f.control(`inbox-${f.core.chat.id}-archive`).onPress());
    assert.equal(f.has(`inbox-chat-${f.core.chat.id}`), true);
    assert.equal(f.has('inbox-undo'), false);
    const change = f.calls.find((c) => c.op === 'chat_layout' && c.input.kind === 'change').input;
    await f.act(async () => {
      ack.resolve(await f.core.canonical(change));
      await ack.promise;
    });
    assert.equal(f.has(`inbox-chat-${f.core.chat.id}`), false);
    assert.equal(f.has('inbox-undo'), true);
  } finally {
    await f.close();
  }
});

test('leading Pin and accessibility Read/Unmute dispatch real canonical actions through the same controller', async () => {
  const f = await inboxUi('pt-BR');
  try {
    const id = f.core.chat.id;
    await f.reveal(id, 'leading');
    await f.act(() => f.control(`inbox-${id}-pin`).onPress());
    assert.equal(f.core.chat.pinned, true);
    assert.equal(f.core.chat.muted_until_ms, '1900000000000');
    await f.act(() =>
      f.control(`inbox-chat-${id}`).onAccessibilityAction({ nativeEvent: { actionName: 'read' } }),
    );
    assert.equal(f.core.chat.unread, 0);
    await f.act(() =>
      f.control(`inbox-chat-${id}`).onAccessibilityAction({ nativeEvent: { actionName: 'mute' } }),
    );
    const mute = f.calls.find((c) => c.op === 'set_chat_mute');
    assert.equal(mute.expected_muted_until_ms, '1900000000000');
    assert.deepEqual(JSON.parse(JSON.stringify(mute.mode)), { kind: 'off' });
    assert.equal(f.core.chat.muted, false);
  } finally {
    await f.close();
  }
});

test('the native long-press menu offers the same named archive action and lost ACK reconciles its exact ID', async () => {
  const f = await inboxUi();
  try {
    f.layout(async (input) => {
      const result = await f.core.canonical(input);
      if (input.kind === 'change') throw Error('Lost response');
      return result;
    });
    const menu = f.control(`inbox-menu-${f.core.chat.id}`);
    assert.equal(menu.shouldOpenOnLongPress, true);
    await f.act(() => menu.onPressAction({ nativeEvent: { event: 'archive' } }));
    const writes = f.calls.filter((c) => c.op === 'chat_layout' && c.input.kind === 'change');
    const reconcile = f.calls.find((c) => c.op === 'chat_layout' && c.input.kind === 'reconcile');
    assert.equal(writes.length, 1);
    assert.equal(reconcile.input.operation_id, writes[0].input.operation_id);
    assert.equal(f.has('inbox-undo'), true);
  } finally {
    await f.close();
  }
});

for (const transition of ['blur', 'background', 'reopen', 'device', 'relay', 'membership'] as const)
  test(`retained swipe/menu/open callbacks cannot adopt ${transition} or dispatch into a new activation`, async () => {
    const f = await inboxUi();
    try {
      await f.reveal(f.core.chat.id, 'trailing');
      const action = f.control(`inbox-${f.core.chat.id}-archive`),
        open = f.control(`inbox-chat-${f.core.chat.id}`);
      await f.act(async () => {
        if (transition === 'blur') {
          f.focus(false);
          f.focus(true);
        } else if (transition === 'background') {
          f.app('background');
          f.app('active');
        } else if (transition === 'reopen') await f.reopen();
        else {
          if (transition === 'device') f.core.snapshot.account!.device_id = 'other-device';
          if (transition === 'relay')
            f.core.snapshot.account!.relay_url = 'https://other-relay.example';
          if (transition === 'membership') f.core.chat.members = [];
          await f.store.refresh();
        }
        action.onPress();
        open.onPress();
      });
      assert.equal(
        f.calls.filter((c) => c.op === 'chat_layout' && c.input.kind === 'change').length,
        0,
      );
      assert.deepEqual(f.routes, []);
    } finally {
      await f.close();
    }
  });

test('archive survives a store cold reopen and appears in Archived; its preview remains unchanged', async () => {
  const f = await inboxUi();
  try {
    const preview = f.core.chat.last_preview,
      id = f.core.chat.id;
    await f.reveal(id, 'trailing');
    await f.act(() => f.control(`inbox-${id}-archive`).onPress());
    await f.act(() => f.reopen());
    assert.equal(f.core.chat.archived, true);
    assert.equal(f.core.chat.last_preview, preview);
    assert.equal(f.has(`inbox-chat-${id}`), false);
    await f.act(() => f.control('inbox-filter-archived').onPress());
    assert.equal(f.has(`inbox-chat-${id}`), true);
    await f.reveal(id, 'trailing');
    assert.equal(
      f.control(`inbox-${id}-archive`).accessibilityLabel,
      f.t('inboxActions.unarchive'),
    );
    await f.act(() => f.control(`inbox-${id}-archive`).onPress());
    assert.equal(f.core.chat.archived, false);
    assert.equal(f.has(`inbox-chat-${id}`), false);
    await f.act(() => f.control('inbox-undo').onPress());
    assert.equal(f.core.chat.archived, true);
    assert.equal(f.has(`inbox-chat-${id}`), true);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('Saved Messages retains inbox actions and routes an unready room through recovery', async () => {
  const f = await inboxUi('pt-BR');
  try {
    Object.assign(f.core.chat, { kind: 'saved', can_write: false, title: '' });
    await f.reopen();
    const row = f.control('saved-messages-chat');
    assert.ok(row.accessibilityLabel.includes(f.t('settings.savedMessages')));
    await f.act(() => row.onPress());
    assert.deepEqual(f.routes.at(-1), '/saved-messages');
    await f.reveal(f.core.chat.id, 'leading');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
