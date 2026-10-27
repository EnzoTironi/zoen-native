import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mutedUi, nodes, settle } from './muted-chats-ui.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const) {
  test(`${locale}: actual native duration options dispatch the selected canonical preset`, async () => {
    const f = await mutedUi(locale);
    try {
      f.route();
      await settle();
      let route = f.route();
      route.props.session.select('chat-0', route.props.ticket);
      route = f.route();
      const rows = nodes(route.tree).find((n) => n.props.testID === 'mute-duration-options')!.props
        .rows;
      assert.equal(rows[0].title, f.t('notifications.muteOneHour'));
      assert.equal(rows[1].title, f.t('notifications.muteTwoDays'));
      assert.equal(rows[2].title, f.t('notifications.muteCustom'));
      rows[1].onPress();
      await settle();
      assert.deepEqual(
        f.calls.filter((c) => c.op === 'set_chat_mute'),
        [
          {
            op: 'set_chat_mute',
            space_id: 'chat-0',
            mode: { kind: 'for', duration_seconds: 172800, from_ms: '1800000000000' },
            expected_muted_until_ms: '9223372036854775807',
            expected_identity_id: f.snapshot.account!.identity_id,
            expected_device_id: f.snapshot.account!.device_id,
          },
        ],
      );
      assert.equal(f.dialogs.length, 0, 'Choosing a duration has no extra confirmation step');
    } finally {
      await f.close();
    }
  });
  test(`${locale}: actual custom native date input applies only the selected absolute deadline`, async () => {
    const f = await mutedUi(locale);
    try {
      f.route();
      await settle();
      let route = f.route();
      route.props.session.select('chat-0', route.props.ticket);
      route = f.route();
      nodes(route.tree)
        .find((n) => n.props.testID === 'mute-duration-options')!
        .props.rows[2].onPress();
      route = f.route();
      const picker = nodes(route.tree).find((n) => n.props.testID === 'mute-until-picker')!;
      assert.equal(picker.type, 'DateTimePicker');
      assert.equal(picker.props.mode, 'datetime');
      assert.equal(picker.props.display, 'spinner');
      assert.equal(picker.props.minimumDate.getTime(), 1800000000000);
      assert.equal(
        f.calls.some((c) => c.op === 'set_chat_mute'),
        false,
      );
      picker.props.onValueChange({}, new Date(1800123456000));
      route = f.route();
      const apply = nodes(route.tree).find((n) => n.props.testID === 'mute-until-apply')!;
      assert.equal(apply.props.disabled, false);
      assert.equal(apply.props.wrapLabel, true);
      assert.equal(apply.props.label.includes('%{'), false);
      apply.props.onPress();
      await settle();
      const writes = f.calls.filter((c) => c.op === 'set_chat_mute');
      assert.equal(writes.length, 1);
      assert.deepEqual(writes[0].mode, { kind: 'until', until_ms: '1800123456000' });
      assert.equal(f.dialogs.length, 0);
    } finally {
      await f.close();
    }
  });
}

test('custom cancellation preserves the canonical setting and a missing Core clock disables timed choices only', async () => {
  const f = await mutedUi();
  try {
    f.route();
    await settle();
    let route = f.route();
    route.props.session.select('chat-0', route.props.ticket);
    route = f.route();
    nodes(route.tree)
      .find((n) => n.props.testID === 'mute-duration-options')!
      .props.rows[2].onPress();
    route = f.route();
    nodes(route.tree)
      .find((n) => n.props.label === f.t('common.cancel'))!
      .props.onPress();
    route = f.route();
    assert.ok(nodes(route.tree).find((n) => n.props.testID === 'mute-duration-options'));
    assert.equal(
      f.calls.some((c) => c.op === 'set_chat_mute'),
      false,
    );
    f.snapshot.clock_ms = '';
    await route.props.session.load(route.props.session.ticket());
    route = f.route();
    route.props.session.select('chat-0', route.props.ticket);
    route = f.route();
    const rows = nodes(route.tree).find((n) => n.props.testID === 'mute-duration-options')!.props
      .rows;
    assert.deepEqual(
      Array.from(rows, (row: any) => row.disabled),
      [true, true, true, false],
    );
  } finally {
    await f.close();
  }
});

test('Android custom duration exposes both native date and time because its datetime adapter is date-only', async () => {
  const f = await mutedUi('pt-BR', 'android');
  try {
    f.route();
    await settle();
    let route = f.route();
    route.props.session.select('chat-0', route.props.ticket);
    route = f.route();
    nodes(route.tree)
      .find((n) => n.props.testID === 'mute-duration-options')!
      .props.rows[2].onPress();
    route = f.route();
    const pickers = nodes(route.tree).filter((n) => n.type === 'DateTimePicker');
    assert.deepEqual(
      pickers.map((n) => n.props.mode),
      ['date', 'time'],
    );
    assert.ok(pickers.every((n) => n.props.presentation === 'inline'));
    pickers[1]!.props.onValueChange({}, new Date(1800007200000));
    nodes(f.route().tree)
      .find((n) => n.props.testID === 'mute-until-apply')!
      .props.onPress();
    await settle();
    assert.deepEqual(f.calls.find((c) => c.op === 'set_chat_mute').mode, {
      kind: 'until',
      until_ms: '1800007200000',
    });
  } finally {
    await f.close();
  }
});
