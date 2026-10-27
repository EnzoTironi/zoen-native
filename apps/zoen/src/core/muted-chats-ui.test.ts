import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mutedUi, nodes, strings, settle } from './muted-chats-ui.fixtures.ts';
import { deferred } from './muted-chats.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: actual You -> notifications entry and named native sheet actions`, async () => {
    const f = await mutedUi(locale);
    const youRows = nodes(f.you())
      .filter((node) => node.type === 'SettingsRows')
      .flatMap((node) => node.props.rows);
    youRows.find((row: any) => row.id === 'settings-notifications').onPress();
    assert.equal(f.routes[0], '/notifications');
    const notificationScreen = f.entry();
    assert.equal(notificationScreen.type, 'ScrollView');
    assert.equal(notificationScreen.props.title, f.t('notifications.notifications'));
    const disclosure = nodes(notificationScreen).find((node) => node.type === 'Collapsible')!;
    assert.equal(disclosure.props.isOpen, false);
    assert.ok(
      strings(disclosure).includes(
        f.t('notifications.registrationConfirmsThatTheServerReceivedTheRequest'),
      ),
    );
    const summary = notificationScreen.props.children[0];
    assert.ok(strings(summary).includes(f.t('notifications.allowedOnDevice')));
    assert.equal(
      strings(summary).includes(
        f.t('notifications.registrationConfirmsThatTheServerReceivedTheRequest'),
      ),
      false,
    );
    disclosure.props.onOpenChange(true);
    assert.equal(nodes(f.entry()).find((node) => node.type === 'Collapsible')!.props.isOpen, true);
    const entry = nodes(notificationScreen).find(
      (node) => node.props.testID === 'notification-chat-settings',
    )!;
    entry.props.rows[0].onPress();
    assert.equal(f.routes[1], '/settings/muted-chats');
    f.route();
    await settle();
    let route = f.route();
    const list = nodes(route.tree).find((node) => node.type === 'FlatList')!;
    assert.equal(list.props.contentInsetAdjustmentBehavior, 'automatic');
    assert.deepEqual(
      Array.from(list.props.data, (row: any) => row.title),
      ['Alpha', 'Beta'],
    );
    const first = list.props.renderItem({ item: list.props.data[0] });
    assert.ok(first.props.rows[0].hint);
    first.props.rows[0].onPress();
    route = f.route();
    const sheet = nodes(route.tree).find((node) => node.type === 'BottomSheet')!;
    assert.equal(sheet.props.isPresented, true);
    assert.equal(nodes(sheet).filter((node) => node.type === 'ScrollView').length, 1);
    const heading = nodes(sheet).find((node) => node.props.accessibilityRole === 'header')!;
    assert.equal(strings(heading), 'Alpha');
    assert.equal(heading.props.numberOfLines, undefined);
    nodes(sheet)
      .find((node) => node.props.testID === 'muted-chat-open')!
      .props.onPress();
    assert.equal(f.routes[2].pathname, '/chat/[id]');
    assert.equal(f.routes[2].params.id, 'chat-0');
    f.confirm(async () => true);
    nodes(sheet)
      .find((node) => node.props.testID === 'muted-chat-unmute')!
      .props.onPress();
    await settle();
    assert.equal(f.dialogs[0].message, f.t('notifications.reenableNamedChat', { name: 'Alpha' }));
    assert.equal(f.dialogs[0].message.includes('%{'), false);
    assert.deepEqual(
      f.calls.filter((command) => command.op === 'set_chat_muted'),
      [
        {
          op: 'set_chat_muted',
          space_id: 'chat-0',
          muted: false,
          expected_identity_id: f.snapshot.account!.identity_id,
          expected_device_id: f.snapshot.account!.device_id,
        },
      ],
    );
    assert.deepEqual(
      Array.from(f.route().props.state.chats, (row: any) => row.id),
      ['chat-1'],
    );
    await f.close();
  });

test('actual hook background/foreground and canonical read retire pending dialogs and stale navigation', async () => {
  const f = await mutedUi();
  f.route();
  await settle();
  let route = f.route();
  route.props.session.select('chat-0', route.props.ticket);
  route = f.route();
  const old = route.props;
  const answer = deferred<boolean>();
  f.confirm(() => answer.promise);
  old.onUnmute(old.state.chats[0]);
  f.foreground('background');
  answer.resolve(true);
  await settle();
  old.onOpen(old.state.chats[0]);
  assert.equal(f.calls.filter((command) => command.op === 'set_chat_muted').length, 0);
  assert.equal(f.routes.length, 0);
  f.snapshot.conversations[0]!.muted = false;
  f.foreground('active');
  await settle();
  route = f.route();
  assert.deepEqual(
    Array.from(route.props.state.chats, (row: any) => row.id),
    ['chat-1'],
  );
  route.props.session.select('chat-1', route.props.ticket);
  route = f.route();
  const next = deferred<boolean>();
  f.confirm(() => next.promise);
  route.props.onUnmute(route.props.state.chats[0]);
  await f.store.refresh();
  next.resolve(true);
  await settle();
  assert.equal(f.calls.filter((command) => command.op === 'set_chat_muted').length, 0);
  assert.equal(f.route().props.state.selected, null);
  f.blur();
  f.foreground('active');
  assert.equal(f.route().props.state.pending, null);
  await f.close();
});

test('compiled loading/error/empty list has read-only retry and no hidden stale sheet', async () => {
  const f = await mutedUi();
  f.driver.operation = async () => {
    throw new Error('offline');
  };
  let route = f.route();
  assert.equal(route.props.state.phase, 'inactive');
  await settle();
  route = f.route();
  assert.equal(route.props.state.phase, 'failed');
  let list = nodes(route.tree).find((node) => node.type === 'FlatList')!;
  assert.equal(list.props.data.length, 0);
  assert.equal(
    nodes(list.props.ListHeaderComponent).some((node) => node.props.accessibilityRole === 'alert'),
    true,
  );
  f.driver.operation = async () => ({ ...f.snapshot, conversations: [] });
  nodes(list.props.ListHeaderComponent)
    .find((node) => node.props.testID === 'muted-chats-retry')!
    .props.onPress();
  await settle();
  route = f.route();
  list = nodes(route.tree).find((node) => node.type === 'FlatList')!;
  assert.equal(route.props.state.phase, 'ready');
  assert.equal(list.props.ListEmptyComponent.props.title, f.t('notifications.noMutedChats'));
  assert.equal(
    nodes(route.tree).find((node) => node.type === 'BottomSheet')!.props.isPresented,
    false,
  );
  await f.close();
});
