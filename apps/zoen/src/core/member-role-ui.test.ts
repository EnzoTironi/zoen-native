import assert from 'node:assert/strict';
import { test } from 'node:test';
import { communityUi, nodes, strings, settle } from './community-members-ui.fixtures.ts';
import { peer, reader, admin, deferred } from './member-role.fixtures.ts';
for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: native menu confirms the person/chat/role and retains canonical display while queued`, async () => {
    const f = await communityUi(locale);
    try {
      f.value({ ...f.context(), role_options: [reader, admin] });
      nodes(f.render().tree)
        .find((node) => node.props.testID === `details-member-${peer}`)!
        .props.onLongPress();
      await settle();
      let screen = f.render();
      const menu = nodes(screen.sheet).find((node) => node.props.testID === 'member-role-menu')!;
      assert.equal(menu.type, 'MenuView');
      assert.equal(
        strings(nodes(screen.sheet).find((node) => node.props.testID === 'member-current-role')),
        f.t('community.member'),
      );
      f.confirm(async () => true);
      menu.props.onPressAction({ nativeEvent: { event: reader.revision } });
      await settle();
      assert.equal(
        f.dialogs[0].title,
        f.t('community.changeRoleNamed', { name: 'Selected', role: f.t('community.reader') }),
      );
      assert.equal(
        f.dialogs[0].message,
        f.t('community.changeRoleIn', { title: 'Selected community' }),
      );
      assert.ok(!JSON.stringify(f.dialogs).includes('%{'));
      const writes = f.calls.filter((call) => call.op === 'set_member_role_reviewed');
      assert.equal(writes.length, 1);
      assert.equal(writes[0].observed_revision, reader.revision);
      assert.equal(writes[0].role, 'reader');
      screen = f.render();
      assert.equal(
        strings(nodes(screen.sheet).find((node) => node.props.testID === 'member-current-role')),
        f.t('community.member'),
      );
      assert.equal(
        nodes(screen.sheet).filter((node) => node.props.testID === 'member-role-menu').length,
        0,
      );
      assert.equal(
        nodes(screen.sheet).filter((node) => node.props.testID === 'member-remove').length,
        0,
      );
      assert.ok(strings(screen.sheet).includes(f.t('community.updating')));
      f.value({
        ...f.context(),
        state: 'Active',
        can_remove: true,
        role: 'reader',
        pending_role: null,
        role_options: [admin],
        revision: '2'.repeat(64),
      });
      f.driver.changed?.({ kind: 'change', spaceIds: [f.room.id] });
      await settle();
      screen = f.render();
      assert.equal(
        strings(nodes(screen.sheet).find((node) => node.props.testID === 'member-current-role')),
        f.t('community.reader'),
      );
    } finally {
      await f.close();
    }
  });
test('actual foreground lifecycle retires a delayed role confirmation', async () => {
  const f = await communityUi(),
    answer = deferred<boolean>();
  try {
    f.value({ ...f.context(), role_options: [reader, admin] });
    nodes(f.render().tree)
      .find((node) => node.props.testID === `details-member-${peer}`)!
      .props.onLongPress();
    await settle();
    const menu = nodes(f.render().sheet).find((node) => node.props.testID === 'member-role-menu')!;
    f.confirm(() => answer.promise);
    menu.props.onPressAction({ nativeEvent: { event: admin.revision } });
    f.foreground('background');
    f.foreground('active');
    answer.resolve(true);
    await settle();
    assert.equal(f.calls.filter((call) => call.op === 'set_member_role_reviewed').length, 0);
    assert.equal(f.render().props!.state.selected, null);
  } finally {
    answer.resolve(false);
    await f.close();
  }
});
test('role refusal shows the canonical role and never retries on read', async () => {
  const f = await communityUi();
  try {
    f.value({ ...f.context(), role_change_failed: true, role_options: [reader, admin] });
    nodes(f.render().tree)
      .find((node) => node.props.testID === `details-member-${peer}`)!
      .props.onLongPress();
    await settle();
    const screen = f.render();
    assert.equal(
      strings(nodes(screen.sheet).find((node) => node.props.accessibilityRole === 'alert')),
      f.t('community.roleRefused'),
    );
    assert.equal(
      strings(nodes(screen.sheet).find((node) => node.props.testID === 'member-current-role')),
      f.t('community.member'),
    );
    assert.equal(f.calls.filter((call) => call.op === 'set_member_role_reviewed').length, 0);
  } finally {
    await f.close();
  }
});
